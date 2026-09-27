# zeroTremor

## Step-by-step: from mouse to safe live test

Use this sequence with the shared `SIGMACHIP Usb Mouse` (`1C4F:0048`). It
keeps calibration, corrected replay, and live input suppression as distinct
steps so that a profile can be checked before it is ever used for cursor
output.

1. List the visible HID devices and identify the demo mouse. Copy its VID/PID
   or, when several devices share the same IDs, its exact path.

   ```bash
   cargo run -p daemon -- devices
   ```

2. Complete the one-time platform permission setup below. On Linux this means
   the HID and `uinput` rules; on macOS grant the terminal Accessibility
   access when the operating system asks.
3. Capture the three calibration sessions and create a profile.

   ```bash
   mkdir -p recordings profiles
   cargo run -p daemon -- capture --vid 1c4f --pid 0048 --record recordings/still.jsonl --segment still
   cargo run -p daemon -- capture --vid 1c4f --pid 0048 --record recordings/slow.jsonl --segment slow
   cargo run -p daemon -- capture --vid 1c4f --pid 0048 --record recordings/flick.jsonl --segment flick
   cargo run -p daemon -- calibrate --still recordings/still.jsonl --slow recordings/slow.jsonl --flick recordings/flick.jsonl --profile profiles/demo-user.json
   ```

   For each capture, do the named movement and press `Ctrl-C`: leave the mouse
   untouched for **still**, make slow deliberate movements for **slow**, and
   make normal quick movements for **flick**.
4. Validate that profile without opening the physical mouse or moving the live
   cursor. First run the dry comparison, then optionally replay its corrected
   output through the platform pointer sink.

   ```bash
   cargo run -p daemon -- replay-filter --record recordings/slow.jsonl --profile profiles/demo-user.json --dry-run
   cargo run -p daemon -- replay-filter --record recordings/slow.jsonl --profile profiles/demo-user.json
   ```

5. On **macOS only**, after validation, test exclusive physical-mouse
   suppression with the live command:

   ```bash
   cargo run -p daemon -- run --vid 1c4f --pid 0048
   # Or: cargo run -p daemon -- run --path '<HID path>'
   ```

   The command opens the selected mouse exclusively, decodes its reports, and
   re-emits synthetic cursor motion. Type `b` then Enter for immediate bypass
   (releases the physical mouse), `e` then Enter to re-enable, or `q`/`Ctrl-C`
   to quit and release it. Keep a keyboard available before starting.

   **Current live-test scope:** `run` deliberately uses `PassthroughFilter`.
   It proves the suppression/re-emission path, but it does **not yet** load
   `demo-user.json` or apply `PersonalizedTremorFilter` to the live mouse.
   Corrected output is currently validated through `replay-filter`.

6. On **Linux**, use corrected replay for cursor-output validation today.
   `run` reports that live suppression is not implemented there; do not expect
   it to suppress the physical mouse.

## Find and inspect a physical mouse

The daemon uses `hidapi`, so these commands are the same on macOS and Linux:

```bash
cargo run -p daemon -- devices
cargo run -p daemon -- capture --vid 1c4f --pid 0048
# Or select the exact path printed by `devices`:
cargo run -p daemon -- capture --path '<HID path>'
```

`devices` prints each visible device's VID/PID, manufacturer, product, and
opaque HID path. `capture` prints each raw input report as timestamped hex
until Ctrl-C.

### Linux: one-time HID permission setup

Run this once for the demo `SIGMACHIP Usb Mouse` (`1c4f:0048`). It creates a
dedicated group and grants that group access only to matching HID nodes.

```bash
sudo groupadd --force zerotremor
sudo usermod -aG zerotremor "$USER"

sudo tee /etc/udev/rules.d/99-zerotremor-mouse.rules >/dev/null <<'EOF'
SUBSYSTEM=="hidraw", KERNEL=="hidraw*", ATTRS{idVendor}=="1c4f", ATTRS{idProduct}=="0048", GROUP="zerotremor", MODE="0660"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

Sign out and back in, then unplug/reconnect the mouse. Confirm that
`id -nG` includes `zerotremor`; after that, run `capture` normally, without
`sudo`. Do not run `sudo cargo`: root has a separate Rustup environment.

### Linux: one-time virtual mouse setup

The replay command creates a virtual mouse through `/dev/uinput`. Load the
kernel module and let the same dedicated group access that node:

```bash
sudo modprobe uinput
sudo tee /etc/udev/rules.d/99-zerotremor-uinput.rules >/dev/null <<'EOF'
KERNEL=="uinput", GROUP="zerotremor", MODE="0660"
EOF
sudo udevadm control --reload-rules
sudo udevadm trigger /dev/uinput
```

After the sign-out/sign-in described above, `ls -l /dev/uinput` should show
group `zerotremor`. To load the module automatically at boot, run
`echo uinput | sudo tee /etc/modules-load.d/uinput.conf`.

### macOS: one-time live-output permission

When you first run `replay-filter` without `--dry-run` or `run`, macOS may
ask for Accessibility permission. Enable the terminal application that runs
Cargo in **System Settings → Privacy & Security → Accessibility**, then stop
and rerun the command. Without that permission zeroTremor fails before it
posts synthetic pointer events.

### Save a capture

Create the destination directory before piping reports into a recording:

```bash
mkdir -p recordings
cargo run -p daemon -- capture --vid 1c4f --pid 0048 | tee recordings/mouse_demo.txt
```

### Personalized calibration recordings

`--record` writes a versioned, OS-neutral JSON Lines (`.jsonl`) recording.
Every file begins with device and report-layout metadata, followed by
timestamped raw reports and decoded buttons, `dx`, `dy`, and wheel values.
The HID path is stored only as local metadata, so the same recording can be
read on Linux and macOS.

```json
{"type":"session","schema_version":1,"platform":"linux","device":{"vendor_id":7231,"product_id":72,"hid_path":"/dev/hidraw1"},"report_layout":"sigmachip_1c4f_0048_v1","segment":"still"}
{"type":"report","seq":1,"t_us":0,"raw_hex":"00 FE 00 00","buttons":0,"dx":-2,"dy":0,"wheel":0}
```

`t_us` is monotonic microseconds since that recording started, which makes it
portable replay timing; it is not an OS-specific clock or cursor position.

Record these three short sessions with the same demo mouse:

```bash
mkdir -p recordings profiles
# Hold the mouse still for 10 seconds, then Ctrl-C.
cargo run -p daemon -- capture --vid 1c4f --pid 0048 --record recordings/still.jsonl --segment still
# Move slowly and deliberately in several directions, then Ctrl-C.
cargo run -p daemon -- capture --vid 1c4f --pid 0048 --record recordings/slow.jsonl --segment slow
# Make several normal fast flicks, then Ctrl-C.
cargo run -p daemon -- capture --vid 1c4f --pid 0048 --record recordings/flick.jsonl --segment flick

cargo run -p daemon -- calibrate \
  --still recordings/still.jsonl \
  --slow recordings/slow.jsonl \
  --flick recordings/flick.jsonl \
  --profile profiles/demo-user.json
```

The generated profile stores the device model, still-hold noise percentile, a
safe deadband cap, slow-motion and flick speeds, a smoothing strength, and a
flick-bypass threshold. The deadband cap is bounded by the lower end of the
slow-intentional recording, so an unusually noisy still hold cannot erase each
small deliberate report. It is an explainable baseline for personalization;
future filtering uses these values rather than assuming every user has the
same tremor pattern.

### Inspect live personalized correction

Load the saved profile and run the causal personalized filter against the same
physical mouse:

```bash
cargo run -p daemon -- filter --vid 1c4f --pid 0048 --profile profiles/demo-user.json
```

This is intentionally diagnostic-only: it prints each decoded report beside
the corrected motion and mode, but does not move the cursor yet. `Deadband`
suppresses movement below the measured still-hold noise; `Smooth` reduces
small rapid reversals; `FlickBypass` preserves a fast intentional flick; and
`PassThrough` leaves likely intentional movement unchanged. The same profile
will later feed the native Linux/macOS pointer sink after the correction is
validated. Flick bypass requires both a high measured speed and a meaningful
single-report displacement, so a tiny movement in a very short report interval
is not accidentally treated as a fast flick.

### Validate corrected replay safely

`replay-filter` is the next integration step. It reads a saved portable
recording, applies `PersonalizedTremorFilter`, and sends **only the corrected
`dx`/`dy` values** to the native virtual pointer sink. It never opens the
physical mouse, so it cannot create duplicate physical-plus-virtual input.

First inspect the comparison without creating a virtual pointer:

```bash
cargo run -p daemon -- replay-filter \
  --record recordings/slow.jsonl \
  --profile profiles/demo-user.json \
  --dry-run
```

Then replay the same corrected output through the virtual mouse. Keep your
hand off the physical mouse while it runs:

```bash
cargo run -p daemon -- replay-filter \
  --record recordings/slow.jsonl \
  --profile profiles/demo-user.json
```

The command prints a per-report raw-versus-corrected table and a summary of
raw/corrected travel distance, net displacement, emitted reports, and how
often each filter mode was selected. Linux uses `zeroTremor Virtual Mouse`
via `/dev/uinput`; macOS uses the CoreGraphics sink. On Linux complete the
one-time `uinput` setup above first; macOS may request Accessibility access.

Use a separate `general` recording for a realistic check of normal work:

```bash
cargo run -p daemon -- capture --vid 1c4f --pid 0048 \
  --record recordings/validation.jsonl --segment general

cargo run -p daemon -- replay-filter \
  --record recordings/validation.jsonl \
  --profile profiles/demo-user.json --dry-run
```

Tune by recapturing—not hand-editing—the three calibration sessions, then
compare the validation summary again. If intentional slow movement is often
deadbanded, redo the **still** hold with the mouse truly untouched and
recalibrate. If tremor-like reversals never show `Smooth`, capture slower,
smaller deliberate movement and re-run calibration. If normal flicks are
reduced, recapture representative flicks. A useful profile suppresses
low-amplitude noise while preserving most validation-recording travel and
fast flicks. Re-run `calibrate` after updating zeroTremor: existing profiles
remain readable, but a new profile includes the current deadband cap and
integer-delta smoothing tuning.

### Telemetry console UI

The React console in `extension/` displays the current device/filter pipeline:
raw and corrected deltas, filter decision, active profile, travel reduction,
and the latest reports. It has two data sources:

- **Validation preview:** deterministic sample data; it never opens a HID
  device or moves the cursor.
- **Native bridge:** real, timestamped HID reports and filter output during a
  calibration session.

For local development:

```bash
cd extension
npm ci
npm run dev
```

For the browser extension build, run `npm run build` and load `extension/dist`
as an unpacked extension. Clicking its toolbar icon opens the telemetry
console. The extension is allowed to contact only the local bridge at
`http://127.0.0.1:8765`.

### Native bridge and UI calibration: complete workflow

The browser cannot open a mouse HID device or run shell commands. The native
bridge performs those privileged tasks; the UI only presents the exercise and
sends target/path context to the bridge. The bridge binds **only** to loopback,
never to the LAN or Internet.

1. Complete the Linux HID permissions setup above, if needed. The bridge
   creates the configured recording directory automatically; no separate
   `mkdir -p recordings profiles` or three terminal `capture` commands are
   required for the UI flow.
2. Start the bridge in one terminal. This command configures the physical
   mouse and the base name for the UI-owned recordings; it does **not** open
   the mouse yet.

   ```bash
   cargo run -p daemon -- bridge \
     --vid 1c4f --pid 0048 \
     --record recordings/demo-user.jsonl
   ```

   Use `--path '<HID path>'` to select one exact device instead of VID/PID.
   Use `--port 8766` only if port `8765` is occupied; the UI currently expects
   port `8765`.
3. Open the telemetry UI and choose **Start full calibration**. It guides the
   user through a still hold, slow deliberate motion, normal flicks, then the
   center target and six precision targets. The timed stages advance
   automatically: 10 seconds still, 15 seconds slow movement, and 10 seconds
   flicks.
   The UI shows an active-capture timer and HID-report count at every stage.
   During a truly untouched still hold, a count of zero is expected and is
   recorded as zero observed still-hold noise.
4. Each of the first three stages opens the selected HID device and writes a
   separate JSONL session. When the flick stage is saved, the bridge derives
   and saves a deterministic profile automatically, then immediately starts
   the target exercise. With the command above it writes:

   ```text
   recordings/demo-user-still.jsonl
   recordings/demo-user-slow.jsonl
   recordings/demo-user-flick.jsonl
   recordings/demo-user-profile.json
   ```

   **Overwrite rule:** these are fresh, per-session files. Starting another
   full calibration with the same `--record recordings/demo-user.jsonl` base
   overwrites all five files above; nothing is appended across sessions. The
   target file is appended only while its own seven-circle stage is active.
   Preserve multiple sessions for ML by changing the base name every time:

   ```bash
   --record recordings/demo-user-session-01.jsonl
   --record recordings/demo-user-session-02.jsonl
   ```

5. The target exercise creates `recordings/demo-user.jsonl`. The UI posts
   pointer-path samples, target geometry, hits, misses, and time-to-target to
   `POST /v1/event`. The bridge stamps every received UI event with the same
   session clock used for HID reports.
6. Completing the final target sends `POST /v1/session/stop`. The capture
   thread stops and flushes the unified recording. The UI can additionally
   download its browser-only JSON copy for inspection.

`Ctrl-C` stops the bridge process. If the UI says **Bridge offline**, start the
bridge command first and confirm no firewall or another process owns port
`8765`. Permission, missing-device, and profile/device-ID mismatch errors are
reported by the bridge terminal; fix them before beginning the exercise.

### Can the UI enable mouse suppression or train ML?

**Not yet.** The UI can start and stop a calibration capture *after you have
started the bridge command yourself*. It shows live raw/corrected telemetry
and stores target-centric data, but a web page or extension cannot safely run
`cargo`, request macOS exclusive HID access, or control the `b`/`e` bypass
switch.

Use the UI for the calibration portion:

1. Start `daemon bridge` from a terminal as shown above.
2. Open the telemetry console and choose **Start calibration**.
3. Complete the center check and six targets; the UI stops and saves the
   bridge recording at the end.
4. Train/recalibrate and validate from the terminal.
5. If using macOS, start the separate `daemon run` command for the live
   suppression proof-of-concept.

The next product step is a small authenticated native companion that exposes
explicit **Enable**, **Bypass**, and **Stop** controls to the UI. Those
controls should call a native service, retain the keyboard bypass, and show
the selected device/profile and permission state before suppression starts.

The UI now automatically creates the **deterministic** profile above; it does
**not** train an ML model automatically. Model training needs multiple
completed calibration sessions and a deliberate train/validation decision, so
it remains an explicit offline step rather than a hidden action after one
exercise.

The local endpoints are intentionally small:

| Endpoint | Used by | Purpose |
| --- | --- | --- |
| `GET /v1/status` | UI or diagnostics | Reports whether a session is active. |
| `POST /v1/session/start` | Begin center check | Opens the configured HID device and begins the session. |
| `GET /v1/telemetry` | UI, polled locally | Returns the latest raw/corrected report and filter mode. |
| `POST /v1/event` | Calibration UI | Persists browser target/path/click context with a bridge timestamp. |
| `POST /v1/session/stop` | Final target | Stops capture and flushes the file. |
| `POST /v1/calibration/derive` | UI after flick stage | Derives and saves the local deterministic profile from still/slow/flick sessions. |

The bridge is a **capture and telemetry service**, not a live cursor-correction
service. It never creates a virtual pointer sink, so it cannot add duplicate
movement while calibration is running.

### Target-centric calibration and coordinates

The calibration contains seven clicks: first the center of the target area,
then targets with different distances, directions, and sizes. Each pointer
path sample includes its browser-local `x/y`, target center and size, target
area dimensions, trial index, current filter telemetry, and whether that
telemetry came from demo data or the native daemon. Click and miss events
include elapsed time and click location.

Raw mouse reports are relative: `dx=-2` means “two units left,” not “the
cursor is at x=-2.” The operating system accumulates those reports. While the
pointer is over the page, the browser provides the resulting `clientX/clientY`
position; the UI subtracts the target-area rectangle to get page-local cursor
coordinates. This lets it know the cursor-to-target relationship without
pretending raw HID reports contain absolute coordinates.

The center target is a repeatable reference and usability check. It validates
the browser coordinate frame and measures an approach to a known target, but
does not calibrate an absolute physical-mouse origin. The bridge's `t_us` is
the authoritative join key: HID reports and UI events are both measured in
microseconds from the same bridge session start. Browser `tMs` remains useful
as UI timing metadata, but should not be used to join streams.

### Unified recording schema

Bridge recordings use JSON Lines. A session header is followed by interleaved
`report` and `target_calibration` events, ordered by the bridge clock:

```json
{"type":"session","schema_version":2,"platform":"linux","device":{"vendor_id":7247,"product_id":72,"hid_path":"/dev/hidraw1"},"report_layout":"sigmachip_1c4f_0048_v1","segment":"general"}
{"type":"report","seq":12,"t_us":184200,"raw_hex":"00 FE 01 00","buttons":0,"dx":-2,"dy":1,"wheel":0,"corrected_dx":0,"corrected_dy":0,"filter_mode":"Deadband"}
{"type":"target_calibration","t_us":185011,"data":{"kind":"pointer_sample","trial":0,"x":418,"y":211,"targetX":435,"targetY":230,"targetSize":58}}
```

`RecordingSession`, raw reports, and profiles are portable across macOS and
Linux. HID paths and browser pixel coordinates are local metadata; model
features should normalize coordinates by `areaWidth`/`areaHeight` and use
device identity only to choose the correct user/device profile.

### How calibration data feeds a machine-learning model

`calibrate` derives explainable statistical values (noise percentile, deadband
cap, smoothing strength, and flick threshold), and
`PersonalizedTremorFilter` applies deterministic rules. The bridge records the
data for a local ML prototype; it does not silently train a model after every
calibration.

The recommended first ML task is a small, causal **intent classifier**, not a
model that directly invents corrected cursor deltas. For a short trailing
window of reports, compute features such as:

- raw and corrected speed, acceleration, direction change, and reversal count;
- timing between reports and movement magnitude;
- normalized cursor-to-target distance and direction;
- target size, trial phase, click/miss, overshoot, and time-to-target;
- current profile thresholds and recent filter modes.

The known target provides supervision unavailable in a raw capture. For
example, a tiny reversal while approaching a distant target may be intentional
correction; repeated small reversals while stationary near a target are more
likely tremor/noise. Labels can begin with exercise labels (`still`, target
approach, precision correction, flick) and objective outcomes (hit/miss,
overshoot). Later, reviewed calibration sessions can add explicit labels for
ambiguous cases.

A practical training pipeline is:

```text
unified JSONL sessions
  → validate schema/device/profile and normalize coordinates
  → build causal windows and feature vectors
  → assign task/outcome labels
  → split train/validation by whole session, not random reports
  → train a small logistic-regression model or shallow tree
  → evaluate hit rate, overshoot, time-to-target, and false suppression
  → export a versioned per-user/device model with its feature schema
```

### Train the local intent-model baseline

The `train` command produces a versioned per-user/per-device nearest-centroid
intent classifier. It learns four conservative classes from one complete UI
bundle: `noise`, `slow_intentional`, `flick`, and (when supplied) target-stage
`precision_correction`.

```bash
cargo run -p daemon -- train \
  --still recordings/demo-user-session-01-still.jsonl \
  --slow recordings/demo-user-session-01-slow.jsonl \
  --flick recordings/demo-user-session-01-flick.jsonl \
  --target recordings/demo-user-session-01.jsonl \
  --model models/demo-user-session-01.json
```

The command creates `models/` when needed and writes feature names, feature
normalization, class centroids, device IDs, and example counts to the model
JSON. It is a transparent first ML baseline—not a claim that one session is a
safe production model, and it is not connected to live cursor correction.

After collecting several complete UI calibrations for the same user and mouse,
take these steps:

1. Keep each calibration bundle together: its `-still.jsonl`, `-slow.jsonl`,
   `-flick.jsonl`, target-exercise `.jsonl`, and generated `-profile.json`.
2. Build examples from **causal** trailing windows of HID reports. Join target
   events by bridge `t_us`; normalize browser coordinates by `areaWidth` and
   `areaHeight`.
3. Label windows with the stage and target outcome: still/noise, slow
   intentional movement, flick, target approach, precision correction,
   hit/miss, overshoot, and time-to-target.
4. Split by complete session, reserving newer sessions for validation. Never
   randomly split adjacent reports from one session across train and test.
5. Train one model from the training sessions, then compare it with the
   deterministic profile on held-out sessions. The current CLI trains one
   bundle at a time; multi-session aggregation and automatic held-out metrics
   are the next trainer extension.
6. Accept a model only when it maintains or improves hit rate and
   time-to-target while reducing unwanted low-amplitude movement. Export the
   model with its feature-schema version, profile/device IDs, metrics, and a
   rollback/bypass path.

This makes the UI’s target data useful without allowing an unvalidated model
to control the cursor. The runtime integration should use model confidence to
choose or tune the existing safety-bounded filter—not let a model directly
invent arbitrary cursor deltas.

At runtime the model would output confidence for classes such as `noise`,
`slow_intentional`, `precision_correction`, and `flick`. The existing safe
filter remains the policy layer: low noise confidence means pass through;
high noise confidence can increase smoothing/deadband; high flick confidence
bypasses correction. Keep hard safety limits—bounded output, causal windows,
profile/device match, and an immediate bypass toggle—outside the ML model.

Train and validate per user and per mouse model first. Do not mix a user's
sessions between training and validation: nearby reports are strongly
correlated and would make accuracy look falsely high. A model is acceptable
only if it preserves or improves target hit rate and time-to-target while
reducing unwanted movement on held-out sessions. Keep recordings local by
default and obtain explicit consent before any cross-user training.

### Replay a recording

```bash
cargo run -p daemon --example replay_mouse -- recordings/mouse_demo.txt
```

The replay example keeps the timing between reports. On macOS it sends the
decoded deltas to the native pointer sink. On Linux it creates a `uinput`
virtual mouse named `zeroTremor Virtual Mouse` and sends the same deltas to
the desktop cursor. The virtual device advertises standard left/right/middle
mouse-button capabilities so Linux desktop input stacks classify it as a
pointer.

### How one replay command selects the correct OS backend

The replay command is identical on both supported platforms:

```bash
cargo run -p daemon --example replay_mouse -- recordings/mouse_demo.txt
```

Rust selects the sink at compile time using `cfg(target_os)`:

```text
Linux  → LinuxUinputPointerSink → /dev/uinput virtual mouse
macOS  → MacOsPointerSink       → CoreGraphics pointer events
```

Only the backend for the machine being built is compiled. A Linux build does
not include the macOS CoreGraphics code, and a macOS build does not include
the Linux `uinput` code. The shared capture decoding, replay timing, and
future tremor filter remain the same on both platforms.

## Concepts and terms

The project separates reading a mouse from deciding what to do with its
movement:

```text
physical mouse → HID report → decoder → PointerSample (dx/dy) → filter → pointer sink → cursor
                         └──────────── replay recording ────────────┘
```

- **HID (Human Interface Device):** the standard protocol used by USB and
  Bluetooth keyboards, mice, gamepads, and similar devices. `hidapi` is the
  cross-platform library this project uses to list and open HID devices.
- **VID/PID:** hexadecimal vendor ID and product ID. Together they identify a
  device model, such as the demo mouse `1C4F:0048`. Several connected devices
  of the same model can share a VID/PID.
- **HID path:** an operating-system-specific identifier for one exact device
  interface, such as `/dev/hidraw1` on Linux. A path should be copied from
  `devices` on the machine where it will be used; it is not portable across
  machines or operating systems.
- **Raw input report:** the bytes sent by the mouse for one event. For the
  current demo mouse, the observed four bytes are buttons, relative X,
  relative Y, and wheel movement. They are shown as hexadecimal values by
  `capture`.
- **Decoder:** code that understands a particular device's report layout and
  turns raw bytes into useful values. A report layout is device-specific, so
  a decoder for the demo mouse must not be assumed to work for every mouse.
- **`dx` / `dy`:** relative movement deltas, not screen coordinates. For
  example, `dx=-2, dy=0` means “move two units left”; it does not mean the
  cursor is at position `(-2, 0)`.
- **`PointerSample`:** the shared Rust value containing decoded `dx`, `dy`,
  and the report timestamp. Filters consume and produce these samples.
- **Replay:** reading a saved capture log and feeding its samples back through
  the pipeline, using the original timing between reports. It makes filter and
  output work reproducible without touching the physical mouse.
- **Corrected replay:** a safe validation mode that applies the saved profile
  to a recording before it reaches the pointer sink. Its comparison summary
  quantifies how much movement was removed before any live correction is
  attempted.
- **Pointer sink:** the final platform-specific component that receives a
  relative movement and asks the OS to move a cursor. A `NoopSink` accepts
  samples but intentionally does nothing, which is useful for safe testing.
- **Virtual pointer sink (Linux):** a pointer sink implemented with Linux
  `uinput`. It creates a virtual mouse device in the kernel and sends it
  relative movement; the desktop treats that virtual device like a mouse.
  `LinuxUinputPointerSink` is this project's implementation.
- **Native pointer sink (macOS):** the macOS implementation uses CoreGraphics
  to post pointer movement. It may require Accessibility permission.
- **udev rule:** a Linux rule that sets access permissions when a device is
  connected. The README rule grants the `zerotremor` group access only to the
  selected demo mouse's `hidraw` interface.

### Live-input safety and platform status

On macOS, `daemon run` is the exclusive-capture proof of concept for the demo
mouse: it suppresses that physical device while it is enabled and releases it
on bypass, quit, Ctrl-C, or a fatal error. It has been verified only with the
`1C4F:0048` demo mouse; other HID report layouts are not supported by this
decoder.

Linux has a virtual pointer sink for replay, but does not yet have selected
physical-mouse suppression. Keep physical-mouse capture and cursor-output
experiments separate there to avoid duplicate input. In either platform's
live testing, retain keyboard access so `b`, `q`, or `Ctrl-C` remains
available.
