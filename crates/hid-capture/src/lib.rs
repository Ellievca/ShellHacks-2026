//! Cross-platform HID discovery and raw-report capture.
//!
//! HID paths are opaque operating-system identifiers. They are printed only
//! for selection and are deliberately not interpreted as filesystem paths.

use core_engine::PointerSample;
use hidapi::{HidApi, HidDevice};
use std::fmt;
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
