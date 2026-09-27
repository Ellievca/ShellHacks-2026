//! Native pointer-output adapters.
//!
//! Linux `uinput` and macOS CoreGraphics implementations belong behind this
//! crate's common interface and are selected with `cfg(target_os)`.

use core_engine::PointerSink;

/// A sink for local development and replay tests before native output is wired.
#[derive(Debug, Default)]
pub struct NoopSink;

impl PointerSink for NoopSink {
    type Error = core::convert::Infallible;

    fn emit_relative(&mut self, _dx: f32, _dy: f32) -> Result<(), Self::Error> {
        Ok(())
    }
}
