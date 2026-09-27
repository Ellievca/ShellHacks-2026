//! zeroTremor command-line entry point.

use hid_capture::{list_devices, open_device, DeviceSelection, DeviceSelector};
use std::env;
use std::io::Write;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
        "capture" | "inspect-device" => capture(parse_selection(rest)?),
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage()),
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

fn capture(selection: DeviceSelection) -> Result<(), String> {
    let opened = open_device(selection).map_err(|error| error.to_string())?;

    let running = Arc::new(AtomicBool::new(true));
    let running_for_handler = Arc::clone(&running);

    ctrlc::set_handler(move || {
        running_for_handler.store(false, Ordering::SeqCst);
    })
    .map_err(|error| format!("could not install Ctrl+C handler: {error}"))?;

    println!(
        "Capturing raw reports from {:04X}:{:04X} {}",
        opened.info.vendor_id, opened.info.product_id, opened.info.path
    );

    println!("Physical mouse is seized while capture is active.");
    println!("Press Ctrl+C to disable and restore normal mouse control.");

    while running.load(Ordering::SeqCst) {
        if let Some(report) = opened.read_raw(100).map_err(|error| error.to_string())? {
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

    println!();
    println!("Filtering disabled.");
    println!("Releasing physical mouse...");

    drop(opened);

    println!("Normal mouse control restored.");

    Ok(())
}

fn parse_selection(args: &[String]) -> Result<DeviceSelection, String> {
    match args {
        [flag, value] if flag == "--path" => Ok(DeviceSelection::Path(value.clone())),
        [vid_flag, vid, pid_flag, pid] if vid_flag == "--vid" && pid_flag == "--pid" => Ok(DeviceSelection::VidPid(DeviceSelector { vendor_id: parse_hex_id(vid, "VID")?, product_id: parse_hex_id(pid, "PID")? })),
        _ => Err("capture requires exactly `--vid 1c4f --pid 0048` or `--path <HID path>`; use `zero-tremor devices` first".into()),
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
    "Usage:\n  zero-tremor devices\n  zero-tremor capture --vid <hex> --pid <hex>\n  zero-tremor capture --path <HID path>\n\nAliases: list-devices, inspect-device".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vid_pid_with_or_without_prefix() {
        assert_eq!(
            parse_selection(&[
                "--vid".into(),
                "0x1c4f".into(),
                "--pid".into(),
                "0048".into()
            ])
            .unwrap(),
            DeviceSelection::VidPid(DeviceSelector {
                vendor_id: 0x1c4f,
                product_id: 0x0048
            })
        );
    }

    #[test]
    fn requires_one_complete_selector() {
        assert!(parse_selection(&["--vid".into(), "1c4f".into()]).is_err());
        assert!(parse_selection(&[
            "--path".into(),
            "/dev/hidraw0".into(),
            "--vid".into(),
            "1c4f".into()
        ])
        .is_err());
    }
}
