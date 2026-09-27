#![allow(clippy::needless_return)]

#[cfg(target_os = "macos")]
use std::error::Error;
#[cfg(target_os = "macos")]
use std::io;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::thread;

#[cfg(target_os = "macos")]
use core_engine::{PassthroughFilter, PointerFilter, PointerSample, PointerSink};
#[cfg(target_os = "macos")]
use hid_capture::{open_device, DeviceSelection, DeviceSelector, OpenedDevice};
#[cfg(target_os = "macos")]
use os_virtual_input::MacOsPointerSink;

#[cfg(target_os = "macos")]
fn open_mouse() -> Result<OpenedDevice, Box<dyn Error>> {
    let selection = DeviceSelection::VidPid(DeviceSelector {
        vendor_id: 0x1c4f,
        product_id: 0x0048,
    });

    Ok(open_device(selection)?)
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn Error>> {
    let running = Arc::new(AtomicBool::new(true));
    let enabled = Arc::new(AtomicBool::new(true));

    let device = Arc::new(Mutex::new(Some(open_mouse()?)));

    let running_for_ctrlc = Arc::clone(&running);

    ctrlc::set_handler(move || {
        running_for_ctrlc.store(false, Ordering::SeqCst);
    })?;

    println!("zeroTremor live passthrough");
    println!("Mouse seized and zeroTremor ENABLED.");
    println!();
    println!("Commands:");
    println!("  b + Enter = bypass / release mouse");
    println!("  e + Enter = enable / seize mouse");
    println!("  q + Enter = quit");
    println!();

    // Keyboard control thread
    {
        let running = Arc::clone(&running);
        let enabled = Arc::clone(&enabled);
        let device = Arc::clone(&device);

        thread::spawn(move || {
            let stdin = io::stdin();

            while running.load(Ordering::SeqCst) {
                let mut input = String::new();

                if stdin.read_line(&mut input).is_err() {
                    continue;
                }

                match input.trim().to_lowercase().as_str() {
                    "b" => {
                        enabled.store(false, Ordering::SeqCst);

                        if let Ok(mut guard) = device.lock() {
                            *guard = None;
                        }

                        println!("BYPASS ENABLED");
                        println!("Physical mouse control restored.");
                    }

                    "e" => {
                        if enabled.load(Ordering::SeqCst) {
                            println!("zeroTremor is already enabled.");
                            continue;
                        }

                        match open_mouse() {
                            Ok(opened) => {
                                if let Ok(mut guard) = device.lock() {
                                    *guard = Some(opened);
                                }

                                enabled.store(true, Ordering::SeqCst);

                                println!("zeroTremor ENABLED");
                                println!("Physical mouse seized.");
                            }

                            Err(error) => {
                                eprintln!("Could not enable zeroTremor: {error}");
                                eprintln!("Remaining in bypass mode.");
                            }
                        }
                    }

                    "q" => {
                        running.store(false, Ordering::SeqCst);
                    }

                    _ => {
                        println!("Use b, e, or q.");
                    }
                }
            }
        });
    }

    let mut filter = PassthroughFilter;
    let mut sink = MacOsPointerSink::new()?;

    while running.load(Ordering::SeqCst) {
        if !enabled.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_millis(20));
            continue;
        }

        let report = {
            let guard = device.lock().map_err(|_| "device lock poisoned")?;

            let Some(opened) = guard.as_ref() else {
                continue;
            };

            opened.read_raw(50)?
        };

        let Some(report) = report else {
            continue;
        };

        if report.bytes.len() < 3 {
            continue;
        }

        let dx = report.bytes[1] as i8 as f32;
        let dy = report.bytes[2] as i8 as f32;

        let timestamp_us = u64::try_from(report.timestamp_us).unwrap_or(u64::MAX);

        let sample = PointerSample {
            dx,
            dy,
            timestamp_us,
        };

        let filtered = filter.filter(sample);

        sink.emit_relative(filtered.dx, filtered.dy)?;
    }

    println!("Shutting down zeroTremor...");
    println!("Releasing physical mouse.");

    if let Ok(mut guard) = device.lock() {
        *guard = None;
    }

    println!("Normal mouse control restored.");

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("live_passthrough is available on macOS only");
}
