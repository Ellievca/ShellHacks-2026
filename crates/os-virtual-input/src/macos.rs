use std::error::Error;
use std::fmt;

use core_engine::PointerSink;

use core_graphics::display::CGDisplay;
use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
}

#[derive(Debug)]
pub struct MacOsSinkError {
    message: String,
}

impl MacOsSinkError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MacOsSinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for MacOsSinkError {}

#[derive(Debug, Default)]
pub struct MacOsPointerSink;

impl MacOsPointerSink {
    pub fn new() -> Result<Self, MacOsSinkError> {
        let allowed = unsafe { CGPreflightPostEventAccess() };

        if !allowed {
            unsafe {
                CGRequestPostEventAccess();
            }

            return Err(MacOsSinkError::new(
                "zeroTremor does not have Accessibility permission. \
                 Open System Settings > Privacy & Security > Accessibility, \
                 enable your terminal application, then restart it and run zeroTremor again.",
            ));
        }

        Ok(Self)
    }

    fn current_cursor_position() -> Result<CGPoint, MacOsSinkError> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| MacOsSinkError::new("Failed to create CoreGraphics event source"))?;

        let event = CGEvent::new(source)
            .map_err(|_| MacOsSinkError::new("Failed to read current cursor position"))?;

        Ok(event.location())
    }

    fn desktop_bounds() -> Result<(f64, f64, f64, f64), MacOsSinkError> {
        let displays = CGDisplay::active_displays().map_err(|error| {
            MacOsSinkError::new(format!("Failed to enumerate macOS displays: {error}"))
        })?;

        if displays.is_empty() {
            return Err(MacOsSinkError::new("No active macOS displays were found"));
        }

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for display_id in displays {
            let bounds = CGDisplay::new(display_id).bounds();

            min_x = min_x.min(bounds.origin.x);
            min_y = min_y.min(bounds.origin.y);
            max_x = max_x.max(bounds.origin.x + bounds.size.width);
            max_y = max_y.max(bounds.origin.y + bounds.size.height);
        }

        Ok((min_x, min_y, max_x, max_y))
    }

    fn move_cursor_to(position: CGPoint) -> Result<(), MacOsSinkError> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| MacOsSinkError::new("Failed to create CoreGraphics event source"))?;

        let event = CGEvent::new_mouse_event(
            source,
            CGEventType::MouseMoved,
            position,
            CGMouseButton::Left,
        )
        .map_err(|_| MacOsSinkError::new("Failed to create synthetic mouse movement event"))?;

        event.post(CGEventTapLocation::HID);

        Ok(())
    }
}

impl PointerSink for MacOsPointerSink {
    type Error = MacOsSinkError;

    fn emit_relative(&mut self, dx: f32, dy: f32) -> Result<(), Self::Error> {
        let current = Self::current_cursor_position()?;

        let (min_x, min_y, max_x, max_y) = Self::desktop_bounds()?;

        let target_x = (current.x + dx as f64).clamp(min_x, max_x - 1.0);

        let target_y = (current.y + dy as f64).clamp(min_y, max_y - 1.0);

        Self::move_cursor_to(CGPoint::new(target_x, target_y))
    }
}
