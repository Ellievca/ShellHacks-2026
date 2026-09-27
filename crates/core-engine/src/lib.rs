//! Platform-neutral pointer processing primitives for zeroTremor.

use serde::{Deserialize, Serialize};
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
    pub slow_speed_p50: f32,
    pub flick_speed_p10: f32,
    pub reversal_window_ms: u32,
    pub smoothing_strength: f32,
    pub flick_speed_threshold: f32,
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
    let slow_speed_p50 = percentile(speeds(slow_reports), 0.50);
    let flick_speed_p10 = percentile(speeds(flick_reports), 0.10);
    Ok(CalibrationProfile {
        schema_version: 1,
        device_vendor_id: still_session.device.vendor_id,
        device_product_id: still_session.device.product_id,
        still_noise_p95,
        slow_speed_p50,
        flick_speed_p10,
        reversal_window_ms: 45,
        smoothing_strength: (still_noise_p95 / (slow_speed_p50 + 1.0)).clamp(0.15, 0.85),
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
