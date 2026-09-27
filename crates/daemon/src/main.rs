//! zeroTremor command-line entry point.

use core_engine::{PassthroughFilter, PointerFilter, PointerSample, PointerSink};
use hid_capture::DeviceSelector;
use os_virtual_input::NoopSink;

fn main() {
    let selector = DeviceSelector {
        vendor_id: 0x1C4F,
        product_id: 0x0048,
    };
    let mut filter = PassthroughFilter;
    let mut sink = NoopSink;
    let sample = PointerSample {
        dx: 0.0,
        dy: 0.0,
        timestamp_us: 0,
    };
    let filtered = filter.filter(sample);

    sink.emit_relative(filtered.dx, filtered.dy)
        .expect("the development sink cannot fail");

    println!(
        "zeroTremor workspace initialized (default device {:04X}:{:04X})",
        selector.vendor_id, selector.product_id
    );
}
