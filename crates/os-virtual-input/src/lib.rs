//! Native pointer-output adapters.

use core_engine::PointerSink;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::{MacOsPointerSink, MacOsSinkError};

#[derive(Debug, Default)]
pub struct NoopSink;

impl PointerSink for NoopSink {
    type Error = core::convert::Infallible;

    fn emit_relative(&mut self, _dx: f32, _dy: f32) -> Result<(), Self::Error> {
        Ok(())
    }
}
