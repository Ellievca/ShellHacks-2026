//! Cross-platform HID discovery and raw-report capture.
//!
//! HID paths are opaque operating-system identifiers. They are printed only
//! for selection and are deliberately not interpreted as filesystem paths.

use core_engine::{
    write_jsonl_event, CalibrationSegment, PointerSample, RecordedReport, RecordingDevice,
    RecordingEvent, RecordingSession, TremorConfig,
};
use hidapi::{HidApi, HidDevice};
use std::fmt;
use std::io::Write;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

/// Wall-clock microseconds since the Unix epoch, as stamped on raw reports.
pub fn now_us() -> Result<u128, SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros())
}

/// Formats report bytes as space-separated uppercase hex, e.g. `00 FF 01 00`.
pub fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Metadata used to select a physical HID device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSelector {
    pub vendor_id: u16,
    pub product_id: u16,
}

/// A device as presented to users by `zero-tremor devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidDeviceInfo {
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
}

impl HidDeviceInfo {
    fn from_hidapi(device: &hidapi::DeviceInfo) -> Self {
        Self {
            product: device.product_string().map(str::to_owned),
            manufacturer: device.manufacturer_string().map(str::to_owned),
            path: device.path().to_string_lossy().into_owned(),
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
        }
    }
}

/// A selection can use the stable vendor/product pair or one exact HID path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelection {
    VidPid(DeviceSelector),
    Path(String),
}

impl fmt::Display for DeviceSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VidPid(selector) => {
                write!(f, "{:04X}:{:04X}", selector.vendor_id, selector.product_id)
            }
            Self::Path(path) => write!(f, "path {path:?}"),
        }
    }
}

/// Errors a caller can render without exposing opaque platform errors alone.
#[derive(Debug)]
pub enum CaptureError {
    HidApiInit(hidapi::HidError),
    DeviceNotFound(DeviceSelection),
    PermissionDenied {
        selection: DeviceSelection,
        source: hidapi::HidError,
    },
    Open {
        selection: DeviceSelection,
        source: hidapi::HidError,
    },
    Read(hidapi::HidError),
    UnsupportedReport(&'static str),
    Clock(std::time::SystemTimeError),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HidApiInit(error) => write!(f, "could not enumerate HID devices: {error}"),
            Self::DeviceNotFound(selection) => write!(f, "no HID device matches {selection}; run `zero-tremor devices` and copy its VID/PID or path"),
            Self::PermissionDenied { selection, source } => write!(f, "permission denied while opening {selection}: {source}. On Linux, grant access to the hidraw device (usually with a udev rule); on macOS, run from an account permitted to access the device."),
            Self::Open { selection, source } => write!(f, "could not open {selection}: {source}"),
            Self::Read(error) => write!(f, "failed while reading HID input report: {error}"),
            Self::UnsupportedReport(reason) => write!(f, "unsupported HID input report: {reason}"),
            Self::Clock(error) => write!(f, "could not timestamp HID input report: {error}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// An opened HID input device. Keep this value alive for the capture session.
pub struct OpenedDevice {
    _api: HidApi,
    device: HidDevice,
    pub info: HidDeviceInfo,
}

/// One unmodified HID input report with a wall-clock timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawInputReport {
    pub timestamp_us: u128,
    pub bytes: Vec<u8>,
}

/// Lists every HID device visible to the current process on macOS and Linux.
pub fn list_devices() -> Result<Vec<HidDeviceInfo>, CaptureError> {
    let api = HidApi::new().map_err(CaptureError::HidApiInit)?;
    Ok(api.device_list().map(HidDeviceInfo::from_hidapi).collect())
}

/// Opens the first device matching a VID/PID, or the exact path selected by a
/// previous `list_devices` invocation.
pub fn open_device(selection: DeviceSelection) -> Result<OpenedDevice, CaptureError> {
    let api = HidApi::new().map_err(CaptureError::HidApiInit)?;
    let selected = api.device_list().find(|device| match &selection {
        DeviceSelection::VidPid(ids) => {
            device.vendor_id() == ids.vendor_id && device.product_id() == ids.product_id
        }
        DeviceSelection::Path(path) => device.path().to_string_lossy().as_ref() == path,
    });
    let device_info = selected.ok_or_else(|| CaptureError::DeviceNotFound(selection.clone()))?;
    let info = HidDeviceInfo::from_hidapi(device_info);
    let device = device_info
        .open_device(&api)
        .map_err(|source| open_error(selection, source))?;

    Ok(OpenedDevice {
        _api: api,
        device,
        info,
    })
}

impl OpenedDevice {
    /// Waits up to `timeout_ms` for one raw report. `Ok(None)` is a timeout.
    pub fn read_raw(&self, timeout_ms: i32) -> Result<Option<RawInputReport>, CaptureError> {
        let mut buffer = [0_u8; 4096];
        let length = self
            .device
            .read_timeout(&mut buffer, timeout_ms)
            .map_err(CaptureError::Read)?;
        if length == 0 {
            return Ok(None);
        }
        if length > buffer.len() {
            return Err(CaptureError::UnsupportedReport(
                "report exceeds the capture buffer",
            ));
        }
        let timestamp_us = now_us().map_err(CaptureError::Clock)?;
        Ok(Some(RawInputReport {
            timestamp_us,
            bytes: buffer[..length].to_vec(),
        }))
    }
}

fn open_error(selection: DeviceSelection, source: hidapi::HidError) -> CaptureError {
    let text = source.to_string().to_ascii_lowercase();
    if text.contains("permission denied") || text.contains("access is denied") {
        CaptureError::PermissionDenied { selection, source }
    } else {
        CaptureError::Open { selection, source }
    }
}

/// One decoded report from a boot-protocol-style mouse such as the Sigmachip
/// 1C4F:0048 (layout observed during PER-35).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseReport {
    /// Bit 0 left, bit 1 right, bit 2 middle.
    pub buttons: u8,
    pub dx: f32,
    pub dy: f32,
    /// Positive scrolls up.
    pub wheel: i8,
}

impl MouseReport {
    /// byte 0 = buttons, bytes 1/2 = signed relative X/Y, byte 3 = optional
    /// signed wheel. Returns `None` for reports shorter than three bytes.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let [buttons, dx, dy, rest @ ..] = bytes else {
            return None;
        };

        Some(Self {
            buttons: *buttons,
            dx: *dx as i8 as f32,
            dy: *dy as i8 as f32,
            wheel: rest.first().map_or(0, |&wheel| wheel as i8),
        })
    }
}

/// Decodes one raw HID input report into a pointer sample.
pub trait ReportDecoder {
    type Error;

    fn decode(&self, report: &[u8], timestamp_us: u64) -> Result<PointerSample, Self::Error>;
}

/// The observed report layout of the shared SIGMACHIP `1C4F:0048` demo mouse.
#[derive(Debug, Default, Clone, Copy)]
pub struct DemoMouseDecoder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoMouseDecodeError {
    UnsupportedLength(usize),
}

impl fmt::Display for DemoMouseDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLength(length) => write!(
                f,
                "expected the demo mouse's four-byte report, received {length} bytes"
            ),
        }
    }
}

impl std::error::Error for DemoMouseDecodeError {}

impl ReportDecoder for DemoMouseDecoder {
    type Error = DemoMouseDecodeError;

    fn decode(&self, report: &[u8], timestamp_us: u64) -> Result<PointerSample, Self::Error> {
        if report.len() != 4 {
            return Err(DemoMouseDecodeError::UnsupportedLength(report.len()));
        }
        Ok(PointerSample {
            dx: report[1] as i8 as f32,
            dy: report[2] as i8 as f32,
            timestamp_us,
        })
    }
}

/// Streams raw reports and their demo-mouse decoding into a portable JSONL file.
pub struct JsonlCaptureRecorder<W: Write> {
    writer: W,
    started_at_us: u128,
    sequence: u64,
}

impl<W: Write> JsonlCaptureRecorder<W> {
    pub fn new(
        writer: W,
        device: &HidDeviceInfo,
        segment: CalibrationSegment,
    ) -> Result<Self, core_engine::RecordingError> {
        Self::start(writer, device, segment, None)
    }

    /// Starts a recording whose session header declares the synthetic tremor
    /// mixed into its reports.
    pub fn new_synthetic(
        writer: W,
        device: &HidDeviceInfo,
        segment: CalibrationSegment,
        tremor: TremorConfig,
    ) -> Result<Self, core_engine::RecordingError> {
        Self::start(writer, device, segment, Some(tremor))
    }

    fn start(
        mut writer: W,
        device: &HidDeviceInfo,
        segment: CalibrationSegment,
        synthetic_tremor: Option<TremorConfig>,
    ) -> Result<Self, core_engine::RecordingError> {
        let started_at_us = now_us()
            .map_err(|error| core_engine::RecordingError::Io(std::io::Error::other(error)))?;
        let session = RecordingSession {
            schema_version: 1,
            platform: std::env::consts::OS.into(),
            device: RecordingDevice {
                vendor_id: device.vendor_id,
                product_id: device.product_id,
                manufacturer: device.manufacturer.clone(),
                product: device.product.clone(),
                hid_path: Some(device.path.clone()),
            },
            report_layout: "sigmachip_1c4f_0048_v1".into(),
            segment,
            synthetic_tremor,
        };
        write_jsonl_event(&mut writer, &RecordingEvent::Session(session))?;
        Ok(Self {
            writer,
            started_at_us,
            sequence: 0,
        })
    }

    pub fn record(&mut self, report: &RawInputReport) -> Result<(), core_engine::RecordingError> {
        self.write_report(report, None)
    }

    /// Records a synthetic-tremor report together with the intentional
    /// movement it was built from, as ground truth.
    pub fn record_synthetic(
        &mut self,
        report: &RawInputReport,
        clean_dx: i8,
        clean_dy: i8,
    ) -> Result<(), core_engine::RecordingError> {
        self.write_report(report, Some((clean_dx, clean_dy)))
    }

    fn write_report(
        &mut self,
        report: &RawInputReport,
        clean: Option<(i8, i8)>,
    ) -> Result<(), core_engine::RecordingError> {
        if report.bytes.len() != 4 {
            return Err(core_engine::RecordingError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "unsupported report: expected 4 bytes, received {}",
                    report.bytes.len()
                ),
            )));
        }
        self.sequence += 1;
        let (clean_dx, clean_dy) = clean.unzip();
        let event = RecordedReport {
            seq: self.sequence,
            t_us: report.timestamp_us.saturating_sub(self.started_at_us) as u64,
            raw_hex: hex_bytes(&report.bytes),
            buttons: report.bytes[0],
            dx: report.bytes[1] as i8,
            dy: report.bytes[2] as i8,
            wheel: report.bytes[3] as i8,
            corrected_dx: None,
            corrected_dy: None,
            filter_mode: None,
            clean_dx,
            clean_dy,
        };
        write_jsonl_event(&mut self.writer, &RecordingEvent::Report(event))
    }
}
