use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

use core_engine::PointerSink;

use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

/// Maps one bit of the HID button byte to its CoreGraphics events.
struct ButtonMapping {
    hid_bit: u8,
    button: CGMouseButton,
    down: CGEventType,
    up: CGEventType,
    dragged: CGEventType,
}

const BUTTONS: [ButtonMapping; 3] = [
    ButtonMapping {
        hid_bit: 0b001,
        button: CGMouseButton::Left,
        down: CGEventType::LeftMouseDown,
        up: CGEventType::LeftMouseUp,
        dragged: CGEventType::LeftMouseDragged,
    },
    ButtonMapping {
        hid_bit: 0b010,
        button: CGMouseButton::Right,
        down: CGEventType::RightMouseDown,
        up: CGEventType::RightMouseUp,
        dragged: CGEventType::RightMouseDragged,
    },
    ButtonMapping {
        hid_bit: 0b100,
        button: CGMouseButton::Center,
        down: CGEventType::OtherMouseDown,
        up: CGEventType::OtherMouseUp,
        dragged: CGEventType::OtherMouseDragged,
    },
];

const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);

/// Generous so double-clicks still register while the cursor is trembling.
const DOUBLE_CLICK_RADIUS_PX: f64 = 10.0;

#[derive(Debug, Clone, Copy)]
struct LastClick {
    hid_bit: u8,
    at: Instant,
    position: CGPoint,
    count: i64,
}

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

pub struct MacOsPointerSink {
    source: CGEventSource,
    /// HID button bits currently held down.
    buttons: u8,
    last_click: Option<LastClick>,
}

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

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| MacOsSinkError::new("Failed to create CoreGraphics event source"))?;

        Ok(Self {
            source,
            buttons: 0,
            last_click: None,
        })
    }

    /// Returns the click count for a new press: consecutive presses of the same
    /// button close together in time and space become double/triple clicks.
    fn register_click(&mut self, hid_bit: u8, position: CGPoint) -> i64 {
        let now = Instant::now();

        let count = match self.last_click {
            Some(last)
                if last.hid_bit == hid_bit
                    && now.duration_since(last.at) <= DOUBLE_CLICK_INTERVAL
                    && (last.position.x - position.x).abs() <= DOUBLE_CLICK_RADIUS_PX
                    && (last.position.y - position.y).abs() <= DOUBLE_CLICK_RADIUS_PX =>
            {
                last.count + 1
            }
            _ => 1,
        };

        self.last_click = Some(LastClick {
            hid_bit,
            at: now,
            position,
            count,
        });

        count
    }

    fn mouse_event(
        &self,
        event_type: CGEventType,
        position: CGPoint,
        button: CGMouseButton,
    ) -> Result<CGEvent, MacOsSinkError> {
        let event = CGEvent::new_mouse_event(self.source.clone(), event_type, position, button)
            .map_err(|_| MacOsSinkError::new("Failed to create synthetic mouse event"))?;

        // CGMouseButton's discriminant is the CoreGraphics button number.
        event.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, button as i64);
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, ZEROTREMOR_EVENT_MARKER);

        Ok(event)
    }

    fn current_cursor_position(&self) -> Result<CGPoint, MacOsSinkError> {
        let event = CGEvent::new(self.source.clone())
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

    /// Moves the cursor, posting a drag event while a button is held so that
    /// click-and-drag (selection, window moves) keeps working.
    fn move_cursor_to(&self, position: CGPoint) -> Result<(), MacOsSinkError> {
        let (event_type, button) = BUTTONS
            .iter()
            .find(|mapping| self.buttons & mapping.hid_bit != 0)
            .map_or((CGEventType::MouseMoved, CGMouseButton::Left), |mapping| {
                (mapping.dragged, mapping.button)
            });

        self.mouse_event(event_type, position, button)?
            .post(CGEventTapLocation::HID);

        Ok(())
    }
}

impl PointerSink for MacOsPointerSink {
    type Error = MacOsSinkError;

    fn emit_relative(&mut self, dx: f32, dy: f32) -> Result<(), Self::Error> {
        let current = self.current_cursor_position()?;

        let (min_x, min_y, max_x, max_y) = Self::desktop_bounds()?;

        let target_x = (current.x + dx as f64).clamp(min_x, max_x - 1.0);

        let target_y = (current.y + dy as f64).clamp(min_y, max_y - 1.0);

        self.move_cursor_to(CGPoint::new(target_x, target_y))
    }

    /// Posts a down or up event for every button whose state changed.
    fn set_buttons(&mut self, buttons: u8) -> Result<(), Self::Error> {
        if buttons == self.buttons {
            return Ok(());
        }

        let position = self.current_cursor_position()?;

        for mapping in &BUTTONS {
            if (buttons ^ self.buttons) & mapping.hid_bit == 0 {
                continue;
            }

            let (event_type, click_count) = if buttons & mapping.hid_bit != 0 {
                (mapping.down, self.register_click(mapping.hid_bit, position))
            } else {
                let count = self
                    .last_click
                    .filter(|click| click.hid_bit == mapping.hid_bit)
                    .map_or(1, |click| click.count);
                (mapping.up, count)
            };

            let event = self.mouse_event(event_type, position, mapping.button)?;
            event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, click_count);
            event.post(CGEventTapLocation::HID);

            self.buttons ^= mapping.hid_bit;
        }

        Ok(())
    }

    fn scroll(&mut self, lines: i32) -> Result<(), Self::Error> {
        if lines == 0 {
            return Ok(());
        }

        let event =
            CGEvent::new_scroll_event(self.source.clone(), ScrollEventUnit::LINE, 1, lines, 0, 0)
                .map_err(|_| MacOsSinkError::new("Failed to create synthetic scroll event"))?;

        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, ZEROTREMOR_EVENT_MARKER);
        event.post(CGEventTapLocation::HID);

        Ok(())
    }
}

impl Drop for MacOsPointerSink {
    /// Never leave a button stuck down, however the caller exits.
    fn drop(&mut self) {
        let _ = self.set_buttons(0);
    }
}

pub const ZEROTREMOR_EVENT_MARKER: i64 = 0x5A54_524D;
