//! Synthetic-tremor capture: records what a trembling hand would have sent,
//! with the intentional movement kept alongside as ground truth.

use core_engine::{PointerSample, TremorConfig, TremorSimulator};
use hid_capture::{now_us, JsonlCaptureRecorder, MouseReport, OpenedDevice, RawInputReport};
use std::fs::File;
use std::thread;
use std::time::{Duration, Instant};

/// The demo mouse's own report interval (125 Hz), so synthetic recordings
/// have the same cadence as physical ones.
const TICK: Duration = Duration::from_millis(8);

/// Physical input gathered during one tick.
struct TickInput {
    dx: i32,
    dy: i32,
    wheel: i8,
    buttons: u8,
}

/// Turns physical input into tremor-affected HID reports, one tick at a time.
struct Synthesizer {
    simulator: TremorSimulator,
    carry: (f32, f32),
    buttons: u8,
}

pub fn capture_with_tremor(
    opened: &OpenedDevice,
    mut recorder: Option<&mut JsonlCaptureRecorder<File>>,
    config: TremorConfig,
) -> Result<(), String> {
    println!(
        "Adding synthetic tremor: {:.1} Hz, {:.1} amplitude. Recorded dx/dy include it; clean_dx/clean_dy hold your intentional movement.",
        config.frequency_hz, config.amplitude_x
    );

    let mut synthesizer = Synthesizer {
        simulator: TremorSimulator::new(config),
        carry: (0.0, 0.0),
        buttons: 0,
    };
    let start = Instant::now();
    let mut next_tick = start;

    loop {
        next_tick += TICK;

        let input = drain_physical_reports(opened, synthesizer.buttons)?;
        let timestamp_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);

        if let Some(report) = synthesizer.tick(&input, timestamp_us)? {
            if let Some(recorder) = recorder.as_mut() {
                recorder
                    .record_synthetic(&report, saturate(input.dx), saturate(input.dy))
                    .map_err(|error| error.to_string())?;
            }
            crate::print_report(&report)?;
        }

        sleep_until(&mut next_tick);
    }
}

impl Synthesizer {
    /// Adds this tick's tremor to the physical input. Returns `None` when
    /// nothing changed, because a real mouse sends nothing then either.
    fn tick(
        &mut self,
        input: &TickInput,
        timestamp_us: u64,
    ) -> Result<Option<RawInputReport>, String> {
        let observed = self
            .simulator
            .inject(PointerSample {
                dx: input.dx as f32,
                dy: input.dy as f32,
                timestamp_us,
            })
            .observed;

        let dx = round_with_carry(observed.dx, &mut self.carry.0);
        let dy = round_with_carry(observed.dy, &mut self.carry.1);
        let buttons_changed = input.buttons != self.buttons;
        self.buttons = input.buttons;

        if dx == 0 && dy == 0 && input.wheel == 0 && !buttons_changed {
            return Ok(None);
        }

        Ok(Some(RawInputReport {
            timestamp_us: now_us()
                .map_err(|error| format!("could not timestamp synthetic report: {error}"))?,
            bytes: vec![input.buttons, dx as u8, dy as u8, input.wheel as u8],
        }))
    }
}

/// Drains queued physical reports. Buttons start at their last known state so
/// a tick without reports doesn't release them.
fn drain_physical_reports(opened: &OpenedDevice, buttons: u8) -> Result<TickInput, String> {
    let mut input = TickInput {
        dx: 0,
        dy: 0,
        wheel: 0,
        buttons,
    };

    for _ in 0..32 {
        let Some(raw) = opened.read_raw(0).map_err(|error| error.to_string())? else {
            break;
        };
        let Some(report) = MouseReport::decode(&raw.bytes) else {
            continue;
        };
        input.dx += report.dx as i32;
        input.dy += report.dy as i32;
        input.wheel = input.wheel.saturating_add(report.wheel);
        input.buttons = report.buttons;
    }

    Ok(input)
}

/// Rounds to a whole HID delta, carrying the remainder into the next tick so
/// sub-pixel tremor is not lost to rounding.
fn round_with_carry(value: f32, carry: &mut f32) -> i8 {
    let total = value + *carry;
    let rounded = total.round().clamp(-127.0, 127.0);
    *carry = total - rounded;
    rounded as i8
}

fn saturate(value: i32) -> i8 {
    value.clamp(-127, 127) as i8
}

fn sleep_until(next_tick: &mut Instant) {
    let now = Instant::now();

    if *next_tick > now {
        thread::sleep(*next_tick - now);
    } else {
        // Don't accumulate timing lag.
        *next_tick = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthesizer() -> Synthesizer {
        Synthesizer {
            simulator: TremorSimulator::new(TremorConfig::new(6.0, 6.0)),
            carry: (0.0, 0.0),
            buttons: 0,
        }
    }

    fn still() -> TickInput {
        TickInput {
            dx: 0,
            dy: 0,
            wheel: 0,
            buttons: 0,
        }
    }

    #[test]
    fn carry_preserves_sub_pixel_tremor() {
        let mut carry = 0.0;
        let total: i32 = (0..10)
            .map(|_| i32::from(round_with_carry(0.3, &mut carry)))
            .sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn large_deltas_saturate_to_hid_range() {
        let mut carry = 0.0;
        assert_eq!(round_with_carry(500.0, &mut carry), 127);
        assert_eq!(saturate(-500), -127);
    }

    #[test]
    fn a_still_hand_still_produces_tremor_reports() {
        let mut synthesizer = synthesizer();
        let reports = (0..125_u64)
            .filter_map(|tick| synthesizer.tick(&still(), tick * 8_000).unwrap())
            .count();
        assert!(reports > 60, "only {reports} reports in one second");
    }

    #[test]
    fn first_tick_adds_no_tremor() {
        assert!(synthesizer().tick(&still(), 0).unwrap().is_none());
    }
}
