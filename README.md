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
until Ctrl-C. If Linux denies access, grant the user access to the matching
`hidraw` device (typically using a narrowly scoped udev rule), then reconnect
the mouse and try again.
