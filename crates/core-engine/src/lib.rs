//! Platform-neutral pointer processing primitives for zeroTremor.

mod tremor_sim;

pub use tremor_sim::{SimulatedPointerSample, TremorConfig, TremorSimulator};

/// A relative pointer movement decoded from a physical input device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerSample {
    pub dx: f32,
    pub dy: f32,
    pub timestamp_us: u64,
}

/// Produces relative pointer movement from a physical device or replay file.
pub trait PointerSource {
    type Error;

    fn next_sample(&mut self) -> Result<PointerSample, Self::Error>;
}

/// Sends relative pointer movement to an operating-system-specific backend.
pub trait PointerSink {
    type Error;

    fn emit_relative(&mut self, dx: f32, dy: f32) -> Result<(), Self::Error>;
}

/// Transforms raw pointer movement into a filtered movement.
pub trait PointerFilter {
    fn filter(&mut self, sample: PointerSample) -> PointerSample;
}

/// Initial pass-through filter. It provides a safe integration baseline before
/// adaptive tremor cancellation is implemented.
#[derive(Debug, Default)]
pub struct PassthroughFilter;

impl PointerFilter for PassthroughFilter {
    fn filter(&mut self, sample: PointerSample) -> PointerSample {
        sample
    }
}
