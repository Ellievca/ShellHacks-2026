use core_engine::{
    write_jsonl_event, CalibrationProfile, CalibrationSegment, FilterMode,
    PersonalizedTremorFilter, RecordedReport, RecordingDevice, RecordingEvent, RecordingSession,
    TargetCalibrationEvent,
};
use hid_capture::{open_device, DemoMouseDecoder, DeviceSelection, ReportDecoder};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

pub fn serve(
    selection: DeviceSelection,
    record_path: String,
    profile_path: Option<String>,
    port: u16,
) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("could not bind native bridge to 127.0.0.1:{port}: {error}"))?;
    let profile = match profile_path {
        Some(path) => Some(
            serde_json::from_reader(
                File::open(&path)
                    .map_err(|error| format!("could not open bridge profile {path:?}: {error}"))?,
            )
            .map_err(|error| format!("invalid bridge profile {path:?}: {error}"))?,
        ),
        None => None,
    };
    let state = Arc::new(BridgeState {
        selection,
        record_path,
        profile,
        active: AtomicBool::new(false),
        sequence: AtomicU64::new(0),
        started: Mutex::new(None),
        writer: Mutex::new(None),
        telemetry: Mutex::new(None),
    });
    println!("zeroTremor native bridge listening on http://127.0.0.1:{port}");
    println!("Open the telemetry UI and choose Start calibration. Ctrl-C stops the bridge.");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    if let Err(error) = handle_request(stream, state) {
                        eprintln!("bridge request error: {error}");
                    }
                });
            }
            Err(error) => eprintln!("bridge connection error: {error}"),
        }
    }
    Ok(())
}

struct BridgeState {
    selection: DeviceSelection,
    record_path: String,
    profile: Option<CalibrationProfile>,
    active: AtomicBool,
    sequence: AtomicU64,
    started: Mutex<Option<Instant>>,
    writer: Mutex<Option<BufWriter<File>>>,
    telemetry: Mutex<Option<serde_json::Value>>,
}

fn handle_request(mut stream: TcpStream, state: Arc<BridgeState>) -> Result<(), String> {
    let (method, path, body) = read_request(&mut stream)?;
    let response = match (method.as_str(), path.as_str()) {
        ("OPTIONS", _) => Ok(serde_json::json!({ "ok": true })),
        ("GET", "/v1/status") => Ok(serde_json::json!({
            "ok": true,
            "active": state.active.load(Ordering::Relaxed),
            "record_path": state.record_path,
        })),
        ("GET", "/v1/telemetry") => Ok(state
            .telemetry
            .lock()
            .map_err(|_| "bridge telemetry lock poisoned")?
            .clone()
            .unwrap_or_else(|| serde_json::json!({ "active": false }))),
        ("POST", "/v1/session/start") => {
            start_session(&state).map(|_| serde_json::json!({ "ok": true }))
        }
        ("POST", "/v1/session/stop") => {
            state.active.store(false, Ordering::SeqCst);
            Ok(serde_json::json!({ "ok": true }))
        }
        ("POST", "/v1/event") => {
            let data = serde_json::from_slice(&body)
                .map_err(|error| format!("invalid bridge event JSON: {error}"))?;
            write_target_event(&state, data).map(|_| serde_json::json!({ "ok": true }))
        }
        _ => Err(format!("unknown bridge endpoint {method} {path}")),
    };
    match response {
        Ok(body) => write_response(&mut stream, "200 OK", &body),
        Err(error) => write_response(
            &mut stream,
            "400 Bad Request",
            &serde_json::json!({ "ok": false, "error": error }),
        ),
    }
}

fn start_session(state: &Arc<BridgeState>) -> Result<(), String> {
    if state.active.swap(true, Ordering::SeqCst) {
        return Err("a calibration session is already active".into());
    }
    let opened = match open_device(state.selection.clone()) {
        Ok(device) => device,
        Err(error) => {
            state.active.store(false, Ordering::SeqCst);
            return Err(error.to_string());
        }
    };
    if let Some(profile) = &state.profile {
        if profile.device_vendor_id != opened.info.vendor_id
            || profile.device_product_id != opened.info.product_id
        {
            state.active.store(false, Ordering::SeqCst);
            return Err(format!(
                "profile is for {:04X}:{:04X}, but bridge device is {:04X}:{:04X}",
                profile.device_vendor_id,
                profile.device_product_id,
                opened.info.vendor_id,
                opened.info.product_id
            ));
        }
    }
    let file = match File::create(&state.record_path) {
        Ok(file) => file,
        Err(error) => {
            state.active.store(false, Ordering::SeqCst);
            return Err(format!(
                "could not create unified recording {:?}: {error}",
                state.record_path
            ));
        }
    };
    let session = RecordingSession {
        schema_version: 2,
        platform: std::env::consts::OS.into(),
        device: RecordingDevice {
            vendor_id: opened.info.vendor_id,
            product_id: opened.info.product_id,
            manufacturer: opened.info.manufacturer.clone(),
            product: opened.info.product.clone(),
            hid_path: Some(opened.info.path.clone()),
        },
        report_layout: "sigmachip_1c4f_0048_v1".into(),
        segment: CalibrationSegment::General,
    };
    let mut writer = BufWriter::new(file);
    write_jsonl_event(&mut writer, &RecordingEvent::Session(session))
        .map_err(|error| error.to_string())?;
    *state
        .writer
        .lock()
        .map_err(|_| "bridge writer lock poisoned")? = Some(writer);
    *state
        .started
        .lock()
        .map_err(|_| "bridge clock lock poisoned")? = Some(Instant::now());
    state.sequence.store(0, Ordering::Relaxed);
    let capture_state = Arc::clone(state);
    thread::spawn(move || capture_loop(opened, capture_state));
    Ok(())
}

fn capture_loop(opened: hid_capture::OpenedDevice, state: Arc<BridgeState>) {
    let decoder = DemoMouseDecoder;
    let mut filter = state.profile.clone().map(PersonalizedTremorFilter::new);
    while state.active.load(Ordering::SeqCst) {
        let report = match opened.read_raw(250) {
            Ok(Some(report)) => report,
            Ok(None) => continue,
            Err(error) => {
                eprintln!("bridge HID capture stopped: {error}");
                break;
            }
        };
        let t_us = elapsed_us(&state).unwrap_or(0);
        let raw = match decoder.decode(&report.bytes, t_us) {
            Ok(sample) => sample,
            Err(error) => {
                eprintln!("bridge ignored unsupported report: {error}");
                continue;
            }
        };
        let (corrected, mode) = match filter.as_mut() {
            Some(filter) => filter.filter_with_mode(raw),
            None => (raw, FilterMode::PassThrough),
        };
        let event = RecordedReport {
            seq: state.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            t_us,
            raw_hex: report
                .bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
            buttons: report.bytes.first().copied().unwrap_or(0),
            dx: raw.dx as i8,
            dy: raw.dy as i8,
            wheel: report.bytes.get(3).copied().unwrap_or(0) as i8,
            corrected_dx: Some(corrected.dx as i8),
            corrected_dy: Some(corrected.dy as i8),
            filter_mode: Some(format!("{mode:?}")),
        };
        if let Ok(mut telemetry) = state.telemetry.lock() {
            *telemetry = Some(serde_json::json!({
                "active": true,
                "timestampUs": t_us,
                "rawDx": raw.dx as i8,
                "rawDy": raw.dy as i8,
                "correctedDx": corrected.dx as i8,
                "correctedDy": corrected.dy as i8,
                "mode": format!("{mode:?}"),
            }));
        }
        if let Err(error) = write_event(&state, RecordingEvent::Report(event)) {
            eprintln!("bridge recording stopped: {error}");
            break;
        }
    }
    state.active.store(false, Ordering::SeqCst);
    if let Ok(mut writer) = state.writer.lock() {
        if let Some(writer) = writer.as_mut() {
            let _ = writer.flush();
        }
        *writer = None;
    }
}

fn write_target_event(state: &BridgeState, data: serde_json::Value) -> Result<(), String> {
    if !state.active.load(Ordering::SeqCst) {
        return Err(
            "no active calibration session; choose Start calibration after the bridge is running"
                .into(),
        );
    }
    write_event(
        state,
        RecordingEvent::TargetCalibration(TargetCalibrationEvent {
            t_us: elapsed_us(state)?,
            data,
        }),
    )
}

fn elapsed_us(state: &BridgeState) -> Result<u64, String> {
    state
        .started
        .lock()
        .map_err(|_| "bridge clock lock poisoned")?
        .as_ref()
        .map(|start| start.elapsed().as_micros() as u64)
        .ok_or_else(|| "no active bridge clock".into())
}

fn write_event(state: &BridgeState, event: RecordingEvent) -> Result<(), String> {
    let mut guard = state
        .writer
        .lock()
        .map_err(|_| "bridge writer lock poisoned")?;
    let writer = guard.as_mut().ok_or("no active recording writer")?;
    write_jsonl_event(writer, &event).map_err(|error| error.to_string())?;
    writer
        .flush()
        .map_err(|error| format!("could not flush unified recording: {error}"))
}

fn read_request(stream: &mut TcpStream) -> Result<(String, String, Vec<u8>), String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|part| part == b"\r\n\r\n") {
            break;
        }
    }
    let header_end = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .ok_or("malformed HTTP request")?
        + 4;
    let header =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| "HTTP header is not UTF-8")?;
    let mut lines = header.lines();
    let first = lines.next().ok_or("missing HTTP request line")?;
    let mut parts = first.split_whitespace();
    let method = parts.next().ok_or("missing method")?.to_owned();
    let path = parts.next().ok_or("missing path")?.to_owned();
    let content_length = lines
        .find_map(|line| {
            line.strip_prefix("Content-Length:")
                .or_else(|| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("incomplete HTTP body".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok((
        method,
        path,
        bytes[header_end..header_end + content_length].to_vec(),
    ))
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    body: &serde_json::Value,
) -> Result<(), String> {
    let body = serde_json::to_string(body).map_err(|error| error.to_string())?;
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).map_err(|error| error.to_string())
}
