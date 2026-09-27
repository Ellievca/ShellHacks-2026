//! Cross-platform HID discovery and raw-report capture.
//!
//! HID paths are opaque operating-system identifiers. They are printed only
//! for selection and are deliberately not interpreted as filesystem paths.

use core_engine::{
    write_jsonl_event, CalibrationSegment, PointerSample, RecordedReport, RecordingDevice,
    RecordingEvent, RecordingSession,
};
use hidapi::{HidApi, HidDevice};
use std::fmt;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

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
        let timestamp_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(CaptureError::Clock)?
            .as_micros();
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

/// Linux-only exclusive grab for the pointer event node associated with a
/// selected hidraw device. Keeping this guard alive suppresses the physical
/// device in the desktop input stack; dropping it restores normal input.
#[cfg(target_os = "linux")]
pub struct LinuxPointerGrab {
    _device: evdev::Device,
    pub event_path: std::path::PathBuf,
}

#[cfg(target_os = "linux")]
pub fn grab_linux_pointer_for_hid_path(hid_path: &str) -> Result<LinuxPointerGrab, String> {
    use evdev::RelativeAxisCode;
    use std::path::Path;

    let node = Path::new(hid_path)
        .file_name()
        .ok_or_else(|| format!("invalid HID path {hid_path:?}"))?;
    let sysfs = Path::new("/sys/class/hidraw").join(node).join("device");
    let mut candidates = Vec::new();
    find_event_nodes(&sysfs, 5, &mut candidates).map_err(|error| {
        format!("could not map {hid_path:?} to a Linux input event device: {error}")
    })?;
    let mut pointer = None;
    for path in candidates {
        let device = match evdev::Device::open(&path) {
            Ok(device) => device,
            Err(_) => continue,
        };
        let axes = device.supported_relative_axes();
        if axes.is_some_and(|axes| {
            axes.contains(RelativeAxisCode::REL_X) && axes.contains(RelativeAxisCode::REL_Y)
        }) {
            if pointer.is_some() {
                return Err(format!("multiple pointer event nodes match {hid_path:?}; select the physical mouse's /dev/input/event* node explicitly (Linux event-path selection will be added next)"));
            }
            pointer = Some((path, device));
        }
    }
    let (event_path, mut device) = pointer.ok_or_else(|| format!("no relative pointer event node was found for {hid_path:?}; confirm this is a mouse and that /dev/input/event* permissions are granted"))?;
    device.grab().map_err(|error| format!("could not exclusively grab {event_path:?}: {error}. Grant the zerotremor group access to this /dev/input/event* node"))?;
    Ok(LinuxPointerGrab {
        _device: device,
        event_path,
    })
}

#[cfg(target_os = "linux")]
fn find_event_nodes(
    path: &std::path::Path,
    depth: u8,
    nodes: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    if depth == 0 {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("event") {
            nodes.push(std::path::Path::new("/dev/input").join(name));
        }
        if entry.file_type()?.is_dir() {
            find_event_nodes(&entry.path(), depth - 1, nodes)?;
        }
    }
    Ok(())
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
        mut writer: W,
        device: &HidDeviceInfo,
        segment: CalibrationSegment,
    ) -> Result<Self, core_engine::RecordingError> {
        let started_at_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| core_engine::RecordingError::Io(std::io::Error::other(error)))?
            .as_micros();
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
        };
        write_jsonl_event(&mut writer, &RecordingEvent::Session(session))?;
        Ok(Self {
            writer,
            started_at_us,
            sequence: 0,
        })
    }

    pub fn record(&mut self, report: &RawInputReport) -> Result<(), core_engine::RecordingError> {
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
        let event = RecordedReport {
            seq: self.sequence,
            t_us: report.timestamp_us.saturating_sub(self.started_at_us) as u64,
            raw_hex: report
                .bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
            buttons: report.bytes[0],
            dx: report.bytes[1] as i8,
            dy: report.bytes[2] as i8,
            wheel: report.bytes[3] as i8,
            corrected_dx: None,
            corrected_dy: None,
            filter_mode: None,
        };
        write_jsonl_event(&mut self.writer, &RecordingEvent::Report(event))
    }
}
