#[cfg(target_os = "macos")]
use std::{
    env,
    error::Error,
    fs::{create_dir_all, File},
    io::{BufWriter, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use core_engine::{PointerSample, PointerSink, TremorConfig, TremorSimulator};

#[cfg(target_os = "macos")]
use hid_capture::{open_device, DeviceSelection, DeviceSelector};

#[cfg(target_os = "macos")]
use os_virtual_input::MacOsPointerSink;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    let frequency_hz: f32 = args
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(6.0);

    let amplitude: f32 = args
        .get(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(6.0);

    let output_path = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "recordings/synthetic_tremor.csv".to_string());

    create_dir_all("recordings")?;

    let file = File::create(&output_path)?;
    let mut writer = BufWriter::new(file);

    writeln!(
        writer,
        "timestamp_us,frequency_hz,amplitude_px,\
clean_dx,clean_dy,tremor_dx,tremor_dy,\
observed_dx,observed_dy"
    )?;

    let config = TremorConfig {
        frequency_hz,
        amplitude_x: amplitude,
        amplitude_y: amplitude * 0.75,

        // X/Y don't shake perfectly together.
        y_phase_rad: 1.2,

        // Slowly changing tremor intensity.
        amplitude_mod_hz: 0.35,
        amplitude_mod_depth: 0.25,
    };

    let mut simulator = TremorSimulator::new(config);

    let selection = DeviceSelection::VidPid(DeviceSelector {
        vendor_id: 0x1c4f,
        product_id: 0x0048,
    });

    println!("Opening demo mouse 1C4F:0048...");

    let opened = open_device(selection)?;

    println!("Mouse seized.");
    println!(
        "Synthetic tremor: {:.1} Hz, {:.1}px amplitude",
        frequency_hz, amplitude
    );

    println!("Saving data to: {output_path}");
    println!("Press Ctrl+C to stop.");

    let running = Arc::new(AtomicBool::new(true));

    let handler_running = Arc::clone(&running);

    ctrlc::set_handler(move || {
        handler_running.store(false, Ordering::SeqCst);
    })?;

    let mut sink = MacOsPointerSink::new()?;

    // 125 Hz simulator
    let tick = Duration::from_micros(8_000);

    let start = Instant::now();
    let mut next_tick = Instant::now();

    while running.load(Ordering::SeqCst) {
        next_tick += tick;

        // Intentional movement collected from the
        // physical mouse during this 8 ms window.
        let mut clean_dx = 0.0_f32;
        let mut clean_dy = 0.0_f32;

        // Drain queued HID reports.
        for _ in 0..32 {
            let Some(report) = opened.read_raw(0)? else {
                break;
            };

            if report.bytes.len() < 3 {
                continue;
            }

            clean_dx += report.bytes[1] as i8 as f32;

            clean_dy += report.bytes[2] as i8 as f32;
        }

        let timestamp_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);

        let clean = PointerSample {
            dx: clean_dx,
            dy: clean_dy,
            timestamp_us,
        };

        let simulated = simulator.inject(clean);

        writeln!(
            writer,
            "{},{:.3},{:.3},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5}",
            timestamp_us,
            frequency_hz,
            amplitude,
            simulated.clean.dx,
            simulated.clean.dy,
            simulated.tremor_dx,
            simulated.tremor_dy,
            simulated.observed.dx,
            simulated.observed.dy,
        )?;

        sink.emit_relative(simulated.observed.dx, simulated.observed.dy)?;

        let now = Instant::now();

        if next_tick > now {
            thread::sleep(next_tick - now);
        } else {
            // Don't accumulate timing lag.
            next_tick = now;
        }
    }

    writer.flush()?;

    println!();
    println!("Simulation stopped.");
    println!("Mouse released.");
    println!("Saved: {output_path}");

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The live cursor demo is currently implemented for macOS.");
}
