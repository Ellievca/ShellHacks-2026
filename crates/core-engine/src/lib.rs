//! Platform-neutral pointer processing primitives for zeroTremor.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::io::{BufRead, Write};

/// A relative pointer movement decoded from a physical input device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerSample {
    pub dx: f32,
    pub dy: f32,
    pub timestamp_us: u64,
}

/// Versioned, OS-neutral identity metadata for one physical input device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// Opaque, OS-local path. It is metadata only and is never used on replay.
    pub hid_path: Option<String>,
}

/// The user activity being captured for personalized calibration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationSegment {
    Still,
    SlowIntentional,
    Flick,
    General,
}

/// First line of every JSONL capture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingSession {
    pub schema_version: u32,
    pub platform: String,
    pub device: RecordingDevice,
    pub report_layout: String,
    pub segment: CalibrationSegment,
}

/// A raw report plus the normalized fields consumed by replay and filtering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordedReport {
    pub seq: u64,
    /// Monotonic microseconds since this recording started.
    pub t_us: u64,
    pub raw_hex: String,
    pub buttons: u8,
    pub dx: i8,
    pub dy: i8,
    pub wheel: i8,
}

/// One JSON Lines record. New variants can be added without changing old logs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecordingEvent {
    Session(RecordingSession),
    Report(RecordedReport),
}

#[derive(Debug)]
pub enum RecordingError {
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingSession,
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "recording I/O failed: {error}"),
            Self::Json(error) => write!(f, "invalid recording JSON: {error}"),
            Self::MissingSession => write!(f, "recording must start with a session event"),
        }
    }
}

impl std::error::Error for RecordingError {}

/// Writes portable capture events as newline-delimited JSON.
pub fn write_jsonl_event(
    writer: &mut impl Write,
    event: &RecordingEvent,
) -> Result<(), RecordingError> {
    serde_json::to_writer(&mut *writer, event).map_err(RecordingError::Json)?;
    writer.write_all(b"\n").map_err(RecordingError::Io)
}

/// Reads a recording, validates its session header, and returns report events.
pub fn read_jsonl_reports(
    reader: impl BufRead,
) -> Result<(RecordingSession, Vec<RecordedReport>), RecordingError> {
    let mut events = reader.lines();
    let Some(first) = events.next() else {
        return Err(RecordingError::MissingSession);
    };
    let first = first.map_err(RecordingError::Io)?;
    let RecordingEvent::Session(session) =
        serde_json::from_str(&first).map_err(RecordingError::Json)?
    else {
        return Err(RecordingError::MissingSession);
    };
    let mut reports = Vec::new();
    for line in events {
        let event = serde_json::from_str(&line.map_err(RecordingError::Io)?)
            .map_err(RecordingError::Json)?;
        if let RecordingEvent::Report(report) = event {
            reports.push(report);
        }
    }
    Ok((session, reports))
}

/// Personalized parameters derived from the three calibration segments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationProfile {
    pub schema_version: u32,
    pub device_vendor_id: u16,
    pub device_product_id: u16,
    pub still_noise_p95: f32,
    /// Operational deadband, capped so it cannot erase the user's typical
    /// smallest deliberate slow-movement step. Older profiles use 0.75,
    /// which preserves one-unit mouse reports while still suppressing zeros.
    #[serde(default = "default_deadband_threshold")]
    pub deadband_threshold: f32,
    pub slow_speed_p50: f32,
    pub flick_speed_p10: f32,
    pub reversal_window_ms: u32,
    pub smoothing_strength: f32,
    pub flick_speed_threshold: f32,
}

fn default_deadband_threshold() -> f32 {
    0.75
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    EmptySegment(&'static str),
    DeviceMismatch,
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySegment(segment) => write!(f, "{segment} calibration contains no reports"),
            Self::DeviceMismatch => write!(
                f,
                "calibration recordings belong to different device models"
            ),
        }
    }
}

impl std::error::Error for CalibrationError {}

/// Derives conservative, explainable filter parameters from labeled recordings.
pub fn derive_calibration_profile(
    still: (&RecordingSession, &[RecordedReport]),
    slow: (&RecordingSession, &[RecordedReport]),
    flick: (&RecordingSession, &[RecordedReport]),
) -> Result<CalibrationProfile, CalibrationError> {
    let (still_session, still_reports) = still;
    let (slow_session, slow_reports) = slow;
    let (flick_session, flick_reports) = flick;
    if still_reports.is_empty() {
        return Err(CalibrationError::EmptySegment("still"));
    }
    if slow_reports.is_empty() {
        return Err(CalibrationError::EmptySegment("slow intentional movement"));
    }
    if flick_reports.is_empty() {
        return Err(CalibrationError::EmptySegment("flick"));
    }
    let same_device = |other: &RecordingSession| {
        other.device.vendor_id == still_session.device.vendor_id
            && other.device.product_id == still_session.device.product_id
    };
    if !same_device(slow_session) || !same_device(flick_session) {
        return Err(CalibrationError::DeviceMismatch);
    }
    let still_noise_p95 = percentile(magnitudes(still_reports), 0.95);
    let slow_step_p25 = percentile(magnitudes(slow_reports), 0.25);
    let slow_speed_p50 = percentile(speeds(slow_reports), 0.50);
    let flick_speed_p10 = percentile(speeds(flick_reports), 0.10);
    Ok(CalibrationProfile {
        schema_version: 1,
        device_vendor_id: still_session.device.vendor_id,
        device_product_id: still_session.device.product_id,
        still_noise_p95,
        // HID reports are integer deltas. A still capture can contain a few
        // large bumps, but its p95 must never cause ordinary one-unit slow
        // motion to disappear report-by-report.
        deadband_threshold: still_noise_p95.min((slow_step_p25 * 0.75).max(0.5)),
        slow_speed_p50,
        flick_speed_p10,
        reversal_window_ms: 45,
        // At least 35% history is needed for a one-unit reversal to be
        // observably reduced after integer mouse-delta rounding. The ratio
        // still raises smoothing for users whose measured still noise is high
        // relative to their deliberate speed.
        smoothing_strength: (still_noise_p95 / (slow_speed_p50 + 1.0)).clamp(0.35, 0.85),
        flick_speed_threshold: (flick_speed_p10 * 0.75).max(slow_speed_p50 * 1.5),
    })
}

fn magnitudes(reports: &[RecordedReport]) -> Vec<f32> {
    reports
        .iter()
        .map(|report| ((report.dx as f32).powi(2) + (report.dy as f32).powi(2)).sqrt())
        .collect()
}

fn speeds(reports: &[RecordedReport]) -> Vec<f32> {
    reports
        .windows(2)
        .map(|pair| {
            let dt = pair[1].t_us.saturating_sub(pair[0].t_us).max(1) as f32 / 1_000_000.0;
            (((pair[1].dx as f32).powi(2) + (pair[1].dy as f32).powi(2)).sqrt()) / dt
        })
        .collect()
}

fn percentile(mut values: Vec<f32>, fraction: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * fraction).round() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(segment: CalibrationSegment) -> RecordingSession {
        RecordingSession {
            schema_version: 1,
            platform: "linux".into(),
            device: RecordingDevice {
                vendor_id: 0x1c4f,
                product_id: 0x0048,
                manufacturer: Some("SIGMACHIP".into()),
                product: Some("Usb Mouse".into()),
                hid_path: Some("/dev/hidraw1".into()),
            },
            report_layout: "sigmachip_1c4f_0048_v1".into(),
            segment,
        }
    }

    fn report(t_us: u64, dx: i8, dy: i8) -> RecordedReport {
        RecordedReport {
            seq: t_us,
            t_us,
            raw_hex: "00 00 00 00".into(),
            buttons: 0,
            dx,
            dy,
            wheel: 0,
        }
    }

    #[test]
    fn jsonl_round_trip_and_profile_are_portable() {
        let captured_session = session(CalibrationSegment::Still);
        let mut bytes = Vec::new();
        write_jsonl_event(
            &mut bytes,
            &RecordingEvent::Session(captured_session.clone()),
        )
        .unwrap();
        write_jsonl_event(&mut bytes, &RecordingEvent::Report(report(10, 1, 0))).unwrap();
        let (loaded, reports) = read_jsonl_reports(&bytes[..]).unwrap();
        assert_eq!(loaded, captured_session);
        assert_eq!(reports.len(), 1);
        let profile = derive_calibration_profile(
            (
                &session(CalibrationSegment::Still),
                &[report(0, 1, 0), report(10_000, -1, 0)],
            ),
            (
                &session(CalibrationSegment::SlowIntentional),
                &[report(0, 2, 0), report(10_000, 3, 0)],
            ),
            (
                &session(CalibrationSegment::Flick),
                &[report(0, 10, 0), report(10_000, 15, 0)],
            ),
        )
        .unwrap();
        assert!(profile.flick_speed_threshold > profile.slow_speed_p50);
        assert!(profile.deadband_threshold <= profile.still_noise_p95);
        assert!(profile.smoothing_strength >= 0.35);
    }
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

/// The decision made for one sample by [`PersonalizedTremorFilter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    PassThrough,
    Deadband,
    Smooth,
    FlickBypass,
}

/// A causal, profile-driven filter for reducing low-amplitude reversals.
///
/// It intentionally uses only the past `reversal_window_ms` of movement so it
/// can run live without waiting for future reports.
pub struct PersonalizedTremorFilter {
    profile: CalibrationProfile,
    history: VecDeque<PointerSample>,
    previous_output: PointerSample,
    residual_dx: f32,
    residual_dy: f32,
    mode: FilterMode,
}

impl PersonalizedTremorFilter {
    pub fn new(profile: CalibrationProfile) -> Self {
        Self {
            profile,
            history: VecDeque::new(),
            previous_output: PointerSample {
                dx: 0.0,
                dy: 0.0,
                timestamp_us: 0,
            },
            residual_dx: 0.0,
            residual_dy: 0.0,
            mode: FilterMode::PassThrough,
        }
    }

    pub fn last_mode(&self) -> FilterMode {
        self.mode
    }

    pub fn filter_with_mode(&mut self, sample: PointerSample) -> (PointerSample, FilterMode) {
        self.trim_history(sample.timestamp_us);
        let speed = self.speed(sample);
        let magnitude = magnitude(sample.dx, sample.dy);
        // A short report interval can make a one- or two-unit tremor step
        // appear fast. Require both speed and a meaningful single-report
        // displacement before treating it as an intentional flick.
        let flick_min_step = (self.profile.still_noise_p95 * 3.0).max(4.0);
        let mode = if speed >= self.profile.flick_speed_threshold && magnitude >= flick_min_step {
            FilterMode::FlickBypass
        } else if magnitude <= self.profile.deadband_threshold {
            FilterMode::Deadband
        } else if self.has_small_reversal(sample) {
            FilterMode::Smooth
        } else {
            FilterMode::PassThrough
        };
        self.history.push_back(sample);
        let output = match mode {
            FilterMode::PassThrough | FilterMode::FlickBypass => {
                self.residual_dx = 0.0;
                self.residual_dy = 0.0;
                sample
            }
            FilterMode::Deadband => PointerSample {
                dx: 0.0,
                dy: 0.0,
                ..sample
            },
            FilterMode::Smooth => {
                let strength = self.profile.smoothing_strength;
                let dx = sample.dx * (1.0 - strength) + self.previous_output.dx * strength;
                let dy = sample.dy * (1.0 - strength) + self.previous_output.dy * strength;
                self.emit_with_residual(sample.timestamp_us, dx, dy)
            }
        };
        self.previous_output = output;
        self.mode = mode;
        (output, mode)
    }

    fn speed(&self, sample: PointerSample) -> f32 {
        let Some(previous) = self.history.back() else {
            return 0.0;
        };
        let elapsed_s = sample
            .timestamp_us
            .saturating_sub(previous.timestamp_us)
            .max(1) as f32
            / 1_000_000.0;
        magnitude(sample.dx, sample.dy) / elapsed_s
    }

    fn has_small_reversal(&self, sample: PointerSample) -> bool {
        let Some(previous) = self.history.back() else {
            return false;
        };
        let max_tremor_step = self.profile.still_noise_p95 * 2.5 + 1.0;
        let dot = sample.dx * previous.dx + sample.dy * previous.dy;
        dot < 0.0
            && magnitude(sample.dx, sample.dy) <= max_tremor_step
            && magnitude(previous.dx, previous.dy) <= max_tremor_step
    }

    fn trim_history(&mut self, now_us: u64) {
        let window_us = u64::from(self.profile.reversal_window_ms) * 1_000;
        while self
            .history
            .front()
            .is_some_and(|sample| now_us.saturating_sub(sample.timestamp_us) > window_us)
        {
            self.history.pop_front();
        }
    }

    fn emit_with_residual(&mut self, timestamp_us: u64, dx: f32, dy: f32) -> PointerSample {
        let total_dx = dx + self.residual_dx;
        let total_dy = dy + self.residual_dy;
        let emitted_dx = total_dx.round();
        let emitted_dy = total_dy.round();
        self.residual_dx = total_dx - emitted_dx;
        self.residual_dy = total_dy - emitted_dy;
        PointerSample {
            dx: emitted_dx,
            dy: emitted_dy,
            timestamp_us,
        }
    }
}

impl PointerFilter for PersonalizedTremorFilter {
    fn filter(&mut self, sample: PointerSample) -> PointerSample {
        self.filter_with_mode(sample).0
    }
}

fn magnitude(dx: f32, dy: f32) -> f32 {
    (dx.powi(2) + dy.powi(2)).sqrt()
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    #[test]
    fn personalized_filter_deadbands_and_smooths_small_reversals() {
        let profile = CalibrationProfile {
            schema_version: 1,
            device_vendor_id: 0x1c4f,
            device_product_id: 0x0048,
            still_noise_p95: 0.5,
            deadband_threshold: 0.5,
            slow_speed_p50: 10.0,
            flick_speed_p10: 100.0,
            reversal_window_ms: 45,
            smoothing_strength: 0.5,
            flick_speed_threshold: 10_000.0,
        };
        let mut filter = PersonalizedTremorFilter::new(profile);
        let (still, mode) = filter.filter_with_mode(PointerSample {
            dx: 0.2,
            dy: 0.0,
            timestamp_us: 0,
        });
        assert_eq!(mode, FilterMode::Deadband);
        assert_eq!(still.dx, 0.0);
        filter.filter_with_mode(PointerSample {
            dx: 1.0,
            dy: 0.0,
            timestamp_us: 10_000,
        });
        let (_, mode) = filter.filter_with_mode(PointerSample {
            dx: -1.0,
            dy: 0.0,
            timestamp_us: 20_000,
        });
        assert_eq!(mode, FilterMode::Smooth);
    }

    #[test]
    fn a_fast_small_step_is_not_a_flick() {
        let profile = CalibrationProfile {
            schema_version: 1,
            device_vendor_id: 0x1c4f,
            device_product_id: 0x0048,
            still_noise_p95: 0.5,
            deadband_threshold: 0.5,
            slow_speed_p50: 1.0,
            flick_speed_p10: 10.0,
            reversal_window_ms: 45,
            smoothing_strength: 0.5,
            flick_speed_threshold: 5.0,
        };
        let mut filter = PersonalizedTremorFilter::new(profile);
        filter.filter_with_mode(PointerSample {
            dx: 1.0,
            dy: 0.0,
            timestamp_us: 0,
        });
        let (_, mode) = filter.filter_with_mode(PointerSample {
            dx: -2.0,
            dy: 1.0,
            timestamp_us: 8_000,
        });
        assert_ne!(mode, FilterMode::FlickBypass);
    }
}
