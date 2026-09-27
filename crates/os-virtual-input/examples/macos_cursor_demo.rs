#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use core_engine::PointerSink;
#[cfg(target_os = "macos")]
use os_virtual_input::MacOsPointerSink;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting macOS cursor test...");

    let mut sink = MacOsPointerSink::new()?;

    println!("Moving cursor in a square.");

    for _ in 0..100 {
        sink.emit_relative(2.0, 0.0)?;
        thread::sleep(Duration::from_millis(5));
    }

    for _ in 0..100 {
        sink.emit_relative(0.0, 2.0)?;
        thread::sleep(Duration::from_millis(5));
    }

    for _ in 0..100 {
        sink.emit_relative(-2.0, 0.0)?;
        thread::sleep(Duration::from_millis(5));
    }

    for _ in 0..100 {
        sink.emit_relative(0.0, -2.0)?;
        thread::sleep(Duration::from_millis(5));
    }

    println!("Cursor test complete.");

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macos_cursor_demo is only available on macOS");
}
