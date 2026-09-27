use serde_json::{json, Value};
use std::io::{self, Read, Write};

const MAX_INCOMING: usize = 64 * 1024 * 1024;
const MAX_OUTGOING: usize = 1024 * 1024;

fn read_message<R: Read>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut length_bytes = [0_u8; 4];

    match reader.read_exact(&mut length_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    }

    let length = u32::from_ne_bytes(length_bytes) as usize;

    if length > MAX_INCOMING {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native message is too large",
        ));
    }

    let mut buffer = vec![0_u8; length];
    reader.read_exact(&mut buffer)?;

    let message = serde_json::from_slice(&buffer)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    Ok(Some(message))
}

fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    if bytes.len() > MAX_OUTGOING {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native response is too large",
        ));
    }

    let length = u32::try_from(bytes.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "native response length overflow",
        )
    })?;

    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;

    Ok(())
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    let mut input = stdin.lock();
    let mut output = stdout.lock();

    // IMPORTANT: logs must use stderr, not stdout.
    eprintln!("zeroTremor native host started");

    while let Some(message) = read_message(&mut input)? {
        let response = match message.get("type").and_then(Value::as_str) {
            Some("ping") => json!({
                "type": "pong",
                "ok": true,
                "engine": "zeroTremor"
            }),

            Some("get_status") => json!({
                "type": "status",
                "connected": true,
                "engine": "zeroTremor"
            }),

            // Hardcoded until calibration output is wired in.
            Some("get_profile") => json!({
                "type": "profile",
                "enabled": true,
                "tremorAmplitude": 7.4,
                "smoothingStrength": 0.68,
                "deadband": 1.8,
                "flickThreshold": 22.0
            }),

            Some(other) => json!({
                "type": "error",
                "message": format!("Unknown message type: {other}")
            }),

            None => json!({
                "type": "error",
                "message": "Message is missing a type"
            }),
        };

        write_message(&mut output, &response)?;
    }

    Ok(())
}
