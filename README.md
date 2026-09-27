# zeroTremor

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

### Current safety limitation

Reading a HID mouse does not automatically stop the physical mouse from
moving the cursor. Until exclusive capture/suppression is intentionally added
and tested, a future virtual sink could result in both the physical and
virtual mouse affecting the cursor. Keep the physical-mouse capture and
cursor-output experiments separate during development.
