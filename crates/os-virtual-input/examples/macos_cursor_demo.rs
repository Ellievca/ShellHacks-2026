use std::thread;
use std::time::Duration;

use core_engine::PointerSink;
use os_virtual_input::MacOsPointerSink;

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
