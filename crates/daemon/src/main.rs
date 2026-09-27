//! zeroTremor command-line entry point.

mod bridge;

use core_engine::{
    derive_calibration_profile, read_jsonl_reports, CalibrationProfile, CalibrationSegment,
    FilterMode, PersonalizedTremorFilter, PointerFilter, PointerSample, PointerSink,
};
use hid_capture::{
    list_devices, open_device, DemoMouseDecoder, DeviceSelection, DeviceSelector,
    JsonlCaptureRecorder, ReportDecoder,
};
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::io::Write;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

#[cfg(target_os = "linux")]
use os_virtual_input::LinuxUinputPointerSink;
#[cfg(target_os = "macos")]
use os_virtual_input::MacOsPointerSink;
use os_virtual_input::NoopSink;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(usage());
    };
    match command.as_str() {
        "devices" | "list-devices" if rest.is_empty() => print_devices(),
        "capture" | "inspect-device" => capture(parse_capture_request(rest)?),
        "calibrate" => calibrate(rest),
        "filter" => filter_live(parse_filter_request(rest)?),
        "replay-filter" => replay_filtered(parse_replay_request(rest)?),
        "bridge" => {
            let request = parse_bridge_request(rest)?;
            bridge::serve(
                request.selection,
                request.record_path,
                request.profile_path,
                request.port,
            )
        }
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage()),
    }
}

struct ReplayRequest {
    record_path: String,
    profile_path: String,
    dry_run: bool,
}

struct BridgeRequest {
    selection: DeviceSelection,
    record_path: String,
    profile_path: Option<String>,
    port: u16,
}

/// Replays only the filter output from a saved recording. This deliberately
/// has no physical-HID input path: it is the safe integration test before
/// live cursor correction is attempted.
fn replay_filtered(request: ReplayRequest) -> Result<(), String> {
    let recording_file = File::open(&request.record_path).map_err(|error| {
        format!(
            "could not open recording {:?}: {error}",
            request.record_path
        )
    })?;
    let (session, reports) =
        read_jsonl_reports(BufReader::new(recording_file)).map_err(|error| {
            format!(
                "could not read recording {:?}: {error}",
                request.record_path
            )
        })?;
    if reports.is_empty() {
        return Err(format!(
            "recording {:?} contains no reports",
            request.record_path
        ));
    }
    let profile_file = File::open(&request.profile_path)
        .map_err(|error| format!("could not open profile {:?}: {error}", request.profile_path))?;
    let profile: CalibrationProfile = serde_json::from_reader(profile_file).map_err(|error| {
        format!(
            "invalid calibration profile {:?}: {error}",
            request.profile_path
        )
    })?;
    if profile.device_vendor_id != session.device.vendor_id
        || profile.device_product_id != session.device.product_id
    {
        return Err(format!(
            "profile is for {:04X}:{:04X}, but recording is from {:04X}:{:04X}",
            profile.device_vendor_id,
            profile.device_product_id,
            session.device.vendor_id,
            session.device.product_id
        ));
    }

    println!(
        "Validating {} reports from {:?} with {:?}",
        reports.len(),
        request.record_path,
        request.profile_path
    );
    println!(
        "profile: noise_p95={:.2} deadband={:.2} smoothing={:.2} flick_speed_threshold={:.2}",
        profile.still_noise_p95,
        profile.deadband_threshold,
        profile.smoothing_strength,
        profile.flick_speed_threshold
    );
    println!("t_us       raw(dx,dy)  corrected(dx,dy)  mode");

    if request.dry_run {
        println!("Dry run: no virtual mouse will be created.");
        let mut sink = NoopSink;
        let summary = replay_reports(&reports, profile, &mut sink, false)?;
        print_replay_summary(summary);
        return Ok(());
    }

    println!("Starting corrected virtual-pointer replay in 2 seconds. Keep your hand off the physical mouse.");
    thread::sleep(Duration::from_secs(2));

    #[cfg(target_os = "linux")]
    {
        let mut sink = LinuxUinputPointerSink::new().map_err(|error| error.to_string())?;
        let summary = replay_reports(&reports, profile, &mut sink, true)?;
        print_replay_summary(summary);
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let mut sink = MacOsPointerSink::new().map_err(|error| error.to_string())?;
        let summary = replay_reports(&reports, profile, &mut sink, true)?;
        print_replay_summary(summary);
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err("replay-filter currently supports Linux and macOS only; use --dry-run to inspect this recording here".into())
    }
}

#[derive(Default)]
struct ReplaySummary {
    reports: usize,
    emitted_reports: usize,
    raw_distance: f32,
    corrected_distance: f32,
    raw_dx: f32,
    raw_dy: f32,
    corrected_dx: f32,
    corrected_dy: f32,
    deadband: usize,
    smooth: usize,
    flick_bypass: usize,
    pass_through: usize,
}

fn replay_reports<S: PointerSink>(
    reports: &[core_engine::RecordedReport],
    profile: CalibrationProfile,
    sink: &mut S,
    preserve_timing: bool,
) -> Result<ReplaySummary, String>
where
    S::Error: std::fmt::Display,
{
    let mut filter = PersonalizedTremorFilter::new(profile);
    let mut previous_timestamp = None;
    let mut summary = ReplaySummary::default();
    for report in reports {
        if preserve_timing {
            if let Some(previous) = previous_timestamp {
                // Long idle gaps are irrelevant to filter validation and make
                // an accidental stale recording unpleasant to replay.
                let delay_us = report.t_us.saturating_sub(previous).min(1_000_000);
                thread::sleep(Duration::from_micros(delay_us));
            }
        }
        let raw = PointerSample {
            dx: f32::from(report.dx),
            dy: f32::from(report.dy),
            timestamp_us: report.t_us,
        };
        let (corrected, mode) = filter.filter_with_mode(raw);
        println!(
            "{:<10} ({:>4.0},{:>4.0}) ({:>4.0},{:>4.0})  {:?}",
            raw.timestamp_us, raw.dx, raw.dy, corrected.dx, corrected.dy, mode
        );
        if corrected.dx != 0.0 || corrected.dy != 0.0 {
            sink.emit_relative(corrected.dx, corrected.dy)
                .map_err(|error| {
                    format!("could not emit corrected virtual-pointer movement: {error}")
                })?;
            summary.emitted_reports += 1;
        }
        summary.reports += 1;
        summary.raw_distance += raw.dx.hypot(raw.dy);
        summary.corrected_distance += corrected.dx.hypot(corrected.dy);
        summary.raw_dx += raw.dx;
        summary.raw_dy += raw.dy;
        summary.corrected_dx += corrected.dx;
        summary.corrected_dy += corrected.dy;
        match mode {
            FilterMode::Deadband => summary.deadband += 1,
            FilterMode::Smooth => summary.smooth += 1,
            FilterMode::FlickBypass => summary.flick_bypass += 1,
            FilterMode::PassThrough => summary.pass_through += 1,
        }
        previous_timestamp = Some(raw.timestamp_us);
    }
    Ok(summary)
}

fn print_replay_summary(summary: ReplaySummary) {
    let reduction = if summary.raw_distance == 0.0 {
        0.0
    } else {
        (1.0 - summary.corrected_distance / summary.raw_distance) * 100.0
    };
    println!("\nComparison summary:");
    println!(
        "reports={} emitted_corrected={} raw_distance={:.1} corrected_distance={:.1} reduction={reduction:.1}%",
        summary.reports, summary.emitted_reports, summary.raw_distance, summary.corrected_distance
    );
    println!(
        "net raw=({:.0},{:.0}) corrected=({:.0},{:.0})",
        summary.raw_dx, summary.raw_dy, summary.corrected_dx, summary.corrected_dy
    );
    println!(
        "modes: deadband={} smooth={} pass_through={} flick_bypass={}",
        summary.deadband, summary.smooth, summary.pass_through, summary.flick_bypass
    );
}

struct FilterRequest {
    selection: DeviceSelection,
    profile_path: String,
}

fn filter_live(request: FilterRequest) -> Result<(), String> {
    let opened = open_device(request.selection).map_err(|error| error.to_string())?;
    let profile_file = File::open(&request.profile_path)
        .map_err(|error| format!("could not open profile {:?}: {error}", request.profile_path))?;
    let profile: CalibrationProfile = serde_json::from_reader(profile_file).map_err(|error| {
        format!(
            "invalid calibration profile {:?}: {error}",
            request.profile_path
        )
    })?;
    if profile.device_vendor_id != opened.info.vendor_id
        || profile.device_product_id != opened.info.product_id
    {
        return Err(format!(
            "profile is for {:04X}:{:04X}, but selected device is {:04X}:{:04X}",
            profile.device_vendor_id,
            profile.device_product_id,
            opened.info.vendor_id,
            opened.info.product_id
        ));
    }
    let decoder = DemoMouseDecoder;
    println!(
        "Filtering {:04X}:{:04X} with {} (Ctrl-C to stop; output is diagnostic only)",
        opened.info.vendor_id, opened.info.product_id, request.profile_path
    );
    println!(
        "profile: noise_p95={:.2} deadband={:.2} smoothing={:.2} flick_speed_threshold={:.2}",
        profile.still_noise_p95,
        profile.deadband_threshold,
        profile.smoothing_strength,
        profile.flick_speed_threshold
    );
    let mut filter = PersonalizedTremorFilter::new(profile);
    loop {
        if let Some(report) = opened.read_raw(1_000).map_err(|error| error.to_string())? {
            let raw = decoder
                .decode(&report.bytes, report.timestamp_us as u64)
                .map_err(|error| format!("unsupported report: {error}"))?;
            let corrected = filter.filter(raw);
            println!(
                "timestamp_us={} raw_dx={} raw_dy={} corrected_dx={} corrected_dy={} mode={:?}",
                raw.timestamp_us,
                raw.dx,
                raw.dy,
                corrected.dx,
                corrected.dy,
                filter.last_mode()
            );
            std::io::stdout()
                .flush()
                .map_err(|error| format!("could not write filter output: {error}"))?;
        }
    }
}

fn print_devices() -> Result<(), String> {
    let devices = list_devices().map_err(|error| error.to_string())?;
    if devices.is_empty() {
        println!("No HID devices are visible. Check that the mouse is connected and permissions allow HID enumeration.");
        return Ok(());
    }
    println!("VID:PID    Manufacturer                 Product                      Path");
    for device in devices {
        println!(
            "{:04X}:{:04X}  {:<28} {:<28} {}",
            device.vendor_id,
            device.product_id,
            device.manufacturer.as_deref().unwrap_or("<unknown>"),
            device.product.as_deref().unwrap_or("<unknown>"),
            device.path
        );
    }
    Ok(())
}

struct CaptureRequest {
    selection: DeviceSelection,
    record_path: Option<String>,
    segment: CalibrationSegment,
}

fn capture(request: CaptureRequest) -> Result<(), String> {
    let opened = open_device(request.selection).map_err(|error| error.to_string())?;
    let mut recorder = match request.record_path {
        Some(path) => Some(
            JsonlCaptureRecorder::new(
                File::create(&path)
                    .map_err(|error| format!("could not create recording {path:?}: {error}"))?,
                &opened.info,
                request.segment,
            )
            .map_err(|error| error.to_string())?,
        ),
        None => None,
    };
    println!(
        "Capturing raw reports from {:04X}:{:04X} {} (Ctrl-C to stop)",
        opened.info.vendor_id, opened.info.product_id, opened.info.path
    );
    loop {
        if let Some(report) = opened.read_raw(1_000).map_err(|error| error.to_string())? {
            if let Some(recorder) = recorder.as_mut() {
                recorder
                    .record(&report)
                    .map_err(|error| error.to_string())?;
            }
            let bytes = report
                .bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("timestamp_us={} report={bytes}", report.timestamp_us);
            std::io::stdout()
                .flush()
                .map_err(|error| format!("could not write raw report output: {error}"))?;
        }
    }
}

fn calibrate(args: &[String]) -> Result<(), String> {
    let mut still = None;
    let mut slow = None;
    let mut flick = None;
    let mut profile_path = None;
    let mut index = 0;
    while index < args.len() {
        let (flag, value) = args
            .get(index)
            .zip(args.get(index + 1))
            .ok_or("calibrate options require a path")?;
        match flag.as_str() {
            "--still" => still = Some(value),
            "--slow" => slow = Some(value),
            "--flick" => flick = Some(value),
            "--profile" => profile_path = Some(value),
            _ => return Err(format!("unknown calibrate option {flag:?}")),
        }
        index += 2;
    }
    let load = |path: &String| {
        let file = File::open(path)
            .map_err(|error| format!("could not open calibration recording {path:?}: {error}"))?;
        read_jsonl_reports(BufReader::new(file)).map_err(|error| error.to_string())
    };
    let still = load(still.ok_or("calibrate requires --still <file>")?)?;
    let slow = load(slow.ok_or("calibrate requires --slow <file>")?)?;
    let flick = load(flick.ok_or("calibrate requires --flick <file>")?)?;
    let profile = derive_calibration_profile(
        (&still.0, &still.1),
        (&slow.0, &slow.1),
        (&flick.0, &flick.1),
    )
    .map_err(|error| error.to_string())?;
    let output = profile_path.ok_or("calibrate requires --profile <file>")?;
    let file = File::create(output)
        .map_err(|error| format!("could not create profile {output:?}: {error}"))?;
    serde_json::to_writer_pretty(file, &profile)
        .map_err(|error| format!("could not write profile {output:?}: {error}"))?;
    println!("Saved personalized calibration profile to {output}");
    println!(
        "noise_p95={:.2} deadband={:.2} smoothing={:.2} flick_threshold={:.2}",
        profile.still_noise_p95,
        profile.deadband_threshold,
        profile.smoothing_strength,
        profile.flick_speed_threshold
    );
    Ok(())
}

fn parse_capture_request(args: &[String]) -> Result<CaptureRequest, String> {
    let mut path = None;
    let mut vid = None;
    let mut pid = None;
    let mut record_path = None;
    let mut segment = CalibrationSegment::General;
    let mut index = 0;
    while index < args.len() {
        let (flag, value) = args
            .get(index)
            .zip(args.get(index + 1))
            .ok_or("capture options require a value")?;
        match flag.as_str() {
            "--path" => path = Some(value.clone()),
            "--vid" => vid = Some(parse_hex_id(value, "VID")?),
            "--pid" => pid = Some(parse_hex_id(value, "PID")?),
            "--record" => record_path = Some(value.clone()),
            "--segment" => segment = parse_segment(value)?,
            _ => return Err(format!("unknown capture option {flag:?}")),
        }
        index += 2;
    }
    let selection = match (path, vid, pid) {
        (Some(path), None, None) => DeviceSelection::Path(path),
        (None, Some(vendor_id), Some(product_id)) => DeviceSelection::VidPid(DeviceSelector { vendor_id, product_id }),
        _ => return Err("capture requires exactly `--vid 1c4f --pid 0048` or `--path <HID path>`; use `zero-tremor devices` first".into()),
    };
    Ok(CaptureRequest {
        selection,
        record_path,
        segment,
    })
}

fn parse_filter_request(args: &[String]) -> Result<FilterRequest, String> {
    let mut path = None;
    let mut vid = None;
    let mut pid = None;
    let mut profile_path = None;
    let mut index = 0;
    while index < args.len() {
        let (flag, value) = args
            .get(index)
            .zip(args.get(index + 1))
            .ok_or("filter options require a value")?;
        match flag.as_str() {
            "--path" => path = Some(value.clone()),
            "--vid" => vid = Some(parse_hex_id(value, "VID")?),
            "--pid" => pid = Some(parse_hex_id(value, "PID")?),
            "--profile" => profile_path = Some(value.clone()),
            _ => return Err(format!("unknown filter option {flag:?}")),
        }
        index += 2;
    }
    let selection = match (path, vid, pid) {
        (Some(path), None, None) => DeviceSelection::Path(path),
        (None, Some(vendor_id), Some(product_id)) => DeviceSelection::VidPid(DeviceSelector { vendor_id, product_id }),
        _ => return Err("filter requires exactly one device selector: --vid <hex> --pid <hex> or --path <HID path>".into()),
    };
    Ok(FilterRequest {
        selection,
        profile_path: profile_path.ok_or("filter requires --profile <file>")?,
    })
}

fn parse_replay_request(args: &[String]) -> Result<ReplayRequest, String> {
    let mut record_path = None;
    let mut profile_path = None;
    let mut dry_run = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--record" | "--profile" => {
                let flag = &args[index];
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{flag} requires a path"))?
                    .clone();
                if flag == "--record" {
                    record_path = Some(value);
                } else {
                    profile_path = Some(value);
                }
                index += 2;
            }
            flag => return Err(format!("unknown replay-filter option {flag:?}")),
        }
    }
    Ok(ReplayRequest {
        record_path: record_path.ok_or("replay-filter requires --record <file.jsonl>")?,
        profile_path: profile_path.ok_or("replay-filter requires --profile <profile.json>")?,
        dry_run,
    })
}

fn parse_bridge_request(args: &[String]) -> Result<BridgeRequest, String> {
    let mut path = None;
    let mut vid = None;
    let mut pid = None;
    let mut record_path = None;
    let mut profile_path = None;
    let mut port = 8765;
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--path" => path = Some(value.clone()),
            "--vid" => vid = Some(parse_hex_id(value, "VID")?),
            "--pid" => pid = Some(parse_hex_id(value, "PID")?),
            "--record" => record_path = Some(value.clone()),
            "--profile" => profile_path = Some(value.clone()),
            "--port" => {
                port = value
                    .parse()
                    .map_err(|_| "--port must be a valid TCP port")?
            }
            _ => return Err(format!("unknown bridge option {flag:?}")),
        }
        index += 2;
    }
    let selection = match (path, vid, pid) {
        (Some(path), None, None) => DeviceSelection::Path(path),
        (None, Some(vendor_id), Some(product_id)) => {
            DeviceSelection::VidPid(DeviceSelector { vendor_id, product_id })
        }
        _ => return Err("bridge requires exactly one device selector: --vid <hex> --pid <hex> or --path <HID path>".into()),
    };
    Ok(BridgeRequest {
        selection,
        record_path: record_path.ok_or("bridge requires --record <file.jsonl>")?,
        profile_path,
        port,
    })
}

fn parse_segment(value: &str) -> Result<CalibrationSegment, String> {
    match value {
        "still" => Ok(CalibrationSegment::Still),
        "slow" => Ok(CalibrationSegment::SlowIntentional),
        "flick" => Ok(CalibrationSegment::Flick),
        "general" => Ok(CalibrationSegment::General),
        _ => Err("invalid segment; use still, slow, flick, or general".into()),
    }
}

fn parse_hex_id(value: &str, label: &str) -> Result<u16, String> {
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u16::from_str_radix(value, 16)
        .map_err(|_| format!("invalid {label} {value:?}; expected a 1–4 digit hexadecimal value"))
}

fn usage() -> String {
    "Usage:\n  zero-tremor devices\n  zero-tremor capture (--vid <hex> --pid <hex> | --path <HID path>) [--record <file.jsonl> --segment <still|slow|flick|general>]\n  zero-tremor calibrate --still <file> --slow <file> --flick <file> --profile <profile.json>\n  zero-tremor filter (--vid <hex> --pid <hex> | --path <HID path>) --profile <profile.json>\n  zero-tremor replay-filter --record <file.jsonl> --profile <profile.json> [--dry-run]\n  zero-tremor bridge (--vid <hex> --pid <hex> | --path <HID path>) --record <file.jsonl> [--profile <profile.json>] [--port 8765]\n\nAliases: list-devices, inspect-device".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vid_pid_with_or_without_prefix() {
        assert_eq!(
            parse_capture_request(&[
                "--vid".into(),
                "0x1c4f".into(),
                "--pid".into(),
                "0048".into()
            ])
            .unwrap()
            .selection,
            DeviceSelection::VidPid(DeviceSelector {
                vendor_id: 0x1c4f,
                product_id: 0x0048
            })
        );
    }

    #[test]
    fn requires_one_complete_selector() {
        assert!(parse_capture_request(&["--vid".into(), "1c4f".into()]).is_err());
        assert!(parse_capture_request(&[
            "--path".into(),
            "/dev/hidraw0".into(),
            "--vid".into(),
            "1c4f".into()
        ])
        .is_err());
    }

    #[test]
    fn parses_filtered_replay_with_optional_dry_run() {
        let request = parse_replay_request(&[
            "--dry-run".into(),
            "--record".into(),
            "recordings/slow.jsonl".into(),
            "--profile".into(),
            "profiles/demo.json".into(),
        ])
        .unwrap();
        assert!(request.dry_run);
        assert_eq!(request.record_path, "recordings/slow.jsonl");
    }
}
