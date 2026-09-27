//! zeroTremor command-line entry point.

use core_engine::{
    derive_calibration_profile, read_jsonl_reports, CalibrationProfile, CalibrationSegment,
    PersonalizedTremorFilter, PointerFilter,
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
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage()),
    }
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
        "profile: noise_p95={:.2} smoothing={:.2} flick_speed_threshold={:.2}",
        profile.still_noise_p95, profile.smoothing_strength, profile.flick_speed_threshold
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
        "noise_p95={:.2} smoothing={:.2} flick_threshold={:.2}",
        profile.still_noise_p95, profile.smoothing_strength, profile.flick_speed_threshold
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
    "Usage:\n  zero-tremor devices\n  zero-tremor capture (--vid <hex> --pid <hex> | --path <HID path>) [--record <file.jsonl> --segment <still|slow|flick|general>]\n  zero-tremor calibrate --still <file> --slow <file> --flick <file> --profile <profile.json>\n  zero-tremor filter (--vid <hex> --pid <hex> | --path <HID path>) --profile <profile.json>\n\nAliases: list-devices, inspect-device".into()
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
}
