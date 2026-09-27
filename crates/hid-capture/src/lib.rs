//! Cross-platform HID discovery and decoding boundary.
//!
//! `hidapi` and device-specific report decoders will be added here. Keeping
//! them in this crate prevents OS adapters from leaking into the shared engine.

use core_engine::PointerSample;

/// Metadata used to select a physical HID device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSelector {
    pub vendor_id: u16,
    pub product_id: u16,
}

/// Decodes one raw HID input report into a pointer sample.
pub trait ReportDecoder {
    type Error;

    fn decode(&self, report: &[u8], timestamp_us: u64) -> Result<PointerSample, Self::Error>;
}
