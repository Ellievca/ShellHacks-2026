# macOS Input Suppression

zeroTremor suppresses the selected physical mouse on macOS by keeping
the HID device open exclusively through `hidapi`.

While zeroTremor is enabled:

- The selected physical mouse continues producing HID reports for zeroTremor.
- macOS does not apply the mouse's raw movement directly to the system cursor.
- zeroTremor decodes the relative movement and sends synthetic cursor movement
  through the macOS CoreGraphics pointer sink.

## Safety

The HID device is released when:

- bypass mode is enabled,
- Ctrl+C is pressed,
- the program exits,
- or a fatal input/output error occurs.

Releasing the HID handle immediately restores normal physical mouse control.

## macOS POC limitation

This behavior has been verified with the demo Sigmachip USB mouse
(VID `1C4F`, PID `0048`). Other HID mice may expose different report layouts
or behave differently with exclusive access.
