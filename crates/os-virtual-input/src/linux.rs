//! Linux virtual relative mouse backed by `/dev/uinput`.

use core_engine::PointerSink;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};
use std::fmt;

/// Emits relative mouse movement through a Linux uinput virtual device.
pub struct LinuxUinputPointerSink {
    device: evdev::uinput::VirtualDevice,
}

#[derive(Debug)]
pub enum LinuxUinputSinkError {
    Create(std::io::Error),
    Emit(std::io::Error),
    InvalidDelta { axis: &'static str, value: f32 },
}

impl fmt::Display for LinuxUinputSinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create(error) => write!(f, "could not create the Linux uinput virtual mouse: {error}. Ensure the uinput kernel module is loaded and your user can read/write /dev/uinput"),
            Self::Emit(error) => write!(f, "could not emit virtual mouse movement: {error}"),
            Self::InvalidDelta { axis, value } => write!(f, "cannot emit non-finite or out-of-range {axis} delta {value}"),
        }
    }
}

impl std::error::Error for LinuxUinputSinkError {}

impl LinuxUinputPointerSink {
    /// Registers a virtual relative mouse named `zeroTremor Virtual Mouse`.
    pub fn new() -> Result<Self, LinuxUinputSinkError> {
        let mut axes = AttributeSet::<RelativeAxisCode>::new();
        axes.insert(RelativeAxisCode::REL_X);
        axes.insert(RelativeAxisCode::REL_Y);
        axes.insert(RelativeAxisCode::REL_WHEEL);
        // Desktop input stacks use mouse button capabilities, together with
        // REL_X/REL_Y, to classify a uinput device as a pointer.
        let mut keys = AttributeSet::<KeyCode>::new();
        keys.insert(KeyCode::BTN_LEFT);
        keys.insert(KeyCode::BTN_RIGHT);
        keys.insert(KeyCode::BTN_MIDDLE);
        let device = evdev::uinput::VirtualDevice::builder()
            .map_err(LinuxUinputSinkError::Create)?
            .name("zeroTremor Virtual Mouse")
            .with_keys(&keys)
            .map_err(LinuxUinputSinkError::Create)?
            .with_relative_axes(&axes)
            .map_err(LinuxUinputSinkError::Create)?
            .build()
            .map_err(LinuxUinputSinkError::Create)?;
        Ok(Self { device })
    }

    /// Re-emits a physical mouse button while its source event node is grabbed.
    pub fn emit_button(
        &mut self,
        button: KeyCode,
        pressed: bool,
    ) -> Result<(), LinuxUinputSinkError> {
        self.device
            .emit(&[InputEvent::new(
                EventType::KEY.0,
                button.0,
                i32::from(pressed),
            )])
            .map_err(LinuxUinputSinkError::Emit)
    }

    /// Re-emits wheel movement while its source event node is grabbed.
    pub fn emit_wheel(&mut self, delta: i8) -> Result<(), LinuxUinputSinkError> {
        if delta == 0 {
            return Ok(());
        }
        self.device
            .emit(&[InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_WHEEL.0,
                i32::from(delta),
            )])
            .map_err(LinuxUinputSinkError::Emit)
    }
}

impl PointerSink for LinuxUinputPointerSink {
    type Error = LinuxUinputSinkError;

    fn emit_relative(&mut self, dx: f32, dy: f32) -> Result<(), Self::Error> {
        let dx = device_delta("x", dx)?;
        let dy = device_delta("y", dy)?;
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        let mut events = Vec::with_capacity(2);
        if dx != 0 {
            events.push(InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_X.0,
                dx,
            ));
        }
        if dy != 0 {
            events.push(InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_Y.0,
                dy,
            ));
        }
        self.device
            .emit(&events)
            .map_err(LinuxUinputSinkError::Emit)
    }
}

fn device_delta(axis: &'static str, value: f32) -> Result<i32, LinuxUinputSinkError> {
    if !value.is_finite() || value > i32::MAX as f32 || value < i32::MIN as f32 {
        return Err(LinuxUinputSinkError::InvalidDelta { axis, value });
    }
    Ok(value.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_device_deltas_and_rejects_invalid_values() {
        assert_eq!(device_delta("x", 1.6).unwrap(), 2);
        assert!(matches!(
            device_delta("x", f32::NAN),
            Err(LinuxUinputSinkError::InvalidDelta { .. })
        ));
    }
}
