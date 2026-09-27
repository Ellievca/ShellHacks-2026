use std::env;
use std::error::Error;
use std::fs;
use std::thread;
use std::time::Duration;

use core_engine::{PassthroughFilter, PointerFilter, PointerSample, PointerSink};

use os_virtual_input::MacOsPointerSink;

fn parse_capture_line(line: &str) -> Option<PointerSample> {
    let (timestamp_part, report_part) = line.split_once(" report=")?;

    let timestamp_us: u64 = timestamp_part.strip_prefix("timestamp_us=")?.parse().ok()?;

    let bytes: Vec<u8> = report_part
        .split_whitespace()
        .filter_map(|byte| u8::from_str_radix(byte, 16).ok())
        .collect();

    if bytes.len() < 3 {
        return None;
    }

    // Sigmachip 1C4F:0048 report layout observed during PER-35:
    //
    // byte 0 = buttons
    // byte 1 = relative X
    // byte 2 = relative Y
    // byte 3 = wheel
    //
    // dx/dy are signed 8-bit values.
    let dx = bytes[1] as i8 as f32;
    let dy = bytes[2] as i8 as f32;

    Some(PointerSample {
        dx,
        dy,
        timestamp_us,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "recordings/mouse_demo.txt".to_string());

    println!("Loading recording: {path}");

    let contents = fs::read_to_string(&path)?;

    let samples: Vec<PointerSample> = contents.lines().filter_map(parse_capture_line).collect();

    if samples.is_empty() {
        return Err("No valid mouse samples found in recording".into());
    }

    println!("Loaded {} mouse samples.", samples.len());
    println!("Starting replay in 2 seconds...");
    println!("Keep your hand off the physical mouse.");

    thread::sleep(Duration::from_secs(2));

    let mut sink = MacOsPointerSink::new()?;
    let mut filter = PassthroughFilter;

    let mut previous_timestamp: Option<u64> = None;

    for sample in samples {
        if let Some(previous) = previous_timestamp {
            let delay_us = sample.timestamp_us.saturating_sub(previous);

            // Avoid an unexpectedly enormous pause if recording
            // contained a long idle gap.
            let delay_us = delay_us.min(1_000_000);

            thread::sleep(Duration::from_micros(delay_us));
        }

        let filtered = filter.filter(sample);

        sink.emit_relative(filtered.dx, filtered.dy)?;

        previous_timestamp = Some(sample.timestamp_us);
    }

    println!("Replay complete.");

    Ok(())
}
