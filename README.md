# EBC-A20 Battery Tester

This project is WIP!

Cross-platform desktop and browser application to control ZTE Tech EBC-A20
battery tester.

The deployed web app is available at
[mauri.codes/ebc-battery-tester](https://mauri.codes/ebc-battery-tester).

The app is built with Rust using egui and eframe.

## TODO

These are missing features compared to the original Windows software.

- Calibration support.
- Plot saving as an image.
- Data exporting to CSV file.

These are things that requires attention from development perspective

- Add tests.
- Split UI code to smaller pieces.

## Reverse Engineering Notes

To capture new frames, record USB traffic with tshark and filter by device
address (find it with `lsusb`):

```bash
tshark -i usbmon1 -w output.pcap
```

```bash
usb.device_address == 11
```

### Calibration Feature Notes

The calibration dialog has four reference point fields: low voltage, high
voltage, low current and high current. According to the manual the voltage
references should be 1V and 4V. Each field has its own calibrate button, and
pressing OK at the end closes the dialog.

All calibration frames use command byte `0x04` (not yet in the `CommandType`
enum) and follow the same 10-byte frame structure as other commands. The
sub-command byte in the second position selects the operation:

| Sub-cmd | Operation             | Data    |
|---------|-----------------------|---------|
| `0x00`  | Set low voltage ref   | full mV |
| `0x01`  | Set high voltage ref  | full mV |
| `0x02`  | Set low current ref   | full mA |
| `0x03`  | Set high current ref  | full mA |
| `0x04`  | Confirm/close dialog  | —       |

The value is `encode_base240`-encoded in bytes 3–4, using full mV or mA
instead of the mV/10 and mA/10 scaling used by other commands.

```text
[0xfa] [0x04] [sub] [value_h] [value_l] [0x00] [0x00] [0x00] [checksum] [0xf8]
```

Captured frames:

```text
Low voltage  3747mV → fa 04 00 0f 93 00 00 00 98 f8
High voltage 3758mV → fa 04 01 0f 9e 00 00 00 94 f8
Low current  3000mA → fa 04 02 0c 78 00 00 00 72 f8
High current 3001mA → fa 04 03 0c 79 00 00 00 72 f8
Confirm             → fa 04 04 00 00 00 00 00 00 f8
```

The dialog also has a cancel button. When user presses this, the Confirm frame
is not sent. Does this save the actual values in the device or what does it do?

### Timer Sync Notes

Both charge and discharge modes send an elapsed-minutes counter to the device
once per minute using command `0x0A`. The minute count is base240-encoded in
payload bytes 1–2. The device does not respond beyond its normal report frames.
Likely used to keep the device LCD timer in sync with the host.

```text
1 min → fa 0a 00 01 00 00 00 00 0b f8
2 min → fa 0a 00 02 00 00 00 00 08 f8
3 min → fa 0a 00 03 00 00 00 00 09 f8
```

### Constant Current Charge Notes

Settings cannot be adjusted while charging is active.

Resume charge with 3A, 4.2V, cutoff 0.1A uses command `0x28`:

```text
fa 28 01 3c 01 b4 00 0a aa f8
```

### Discharge Constant Current Notes

Settings can be adjusted on the fly using command `0x07`
(`AdjustDischargeConstantCurrent`). Encoding is the same as the start command:
current in mA/10, cutoff voltage in mV/10, time in minutes, all base240.

```text
200mA, 3.3V cutoff, no time limit → fa 07 00 14 01 5a 00 00 48 f8
```

Resume after stop uses command `0x08`. Despite the current `StopConstantCurrentDischarge`
label in the code, the captured frame includes current, cutoff voltage and time
parameters — it behaves like a resume/continue rather than a plain stop.

```text
100mA, 3.3V cutoff, no time limit → fa 08 00 0a 01 5a 00 00 59 f8
```

### Discharge Constant Power Notes

Settings cannot be adjusted on the fly in this mode. Resume after stop uses
command `0x18` (`Continue`) with power in W, cutoff voltage in mV/10, and time
in minutes. The existing `continue_command()` in the code sends hardcoded zeros
for these parameters and is likely wrong.

```text
1W, 3.3V cutoff, no time limit → fa 18 00 01 01 5a 00 00 42 f8
```

### Internal Resistance Test Notes

The Windows software has a dialog where the user inputs a test current in mA
and presses Test. Command `0x09` triggers a brief discharge at that current.
The current is encoded the same way as discharge commands: mA/10, base240.

```text
 200mA → fa 09 00 14 00 00 00 00 1d f8
1000mA → fa 09 00 64 00 00 00 00 6d f8
2000mA → fa 09 00 c8 00 00 00 00 c1 f8
```

The device responds with a `DischargeConstantPower` off-report (`0x01`) containing
the battery voltage measured under that load, then the fan spins briefly.

```text
fa 01 00 00 0f 93 00 09 00 00 00 01 01 5a 00 00 09 c7 f8
→ voltage=3747mV, current=0mA, mAh=9, power=1W, cutoff=3300mV
```

The Windows software calculates internal resistance from two consecutive voltage
readings: `R = (V_open_circuit − V_under_load) / I`. The pre-test open-circuit
voltage comes from a prior firmware report; the loaded voltage comes from this
response. The "0mR" result means the voltage drop was below the measurement
resolution — either a low-impedance battery or the discharge pulse was too short
to produce a measurable ΔV.

## Important Notes

The browser version uses WebUSB to communicate with the device. WebUSB is only
supported on Chrome, Edge and Opera; for example, the web app will not work with
Firefox.

Only the USB cable that ships with the device is
supported. The cable has a built-in CH340 serial chip, even though the device
end is a mini USB plug, so any regular mini USB cable will not work.

## Access USB Device on Browser

By default WebUSB cannot access the device from the browser as OS driver has
already claimed the device, locking the browser out. In order to use WebUSB, you
need to do additional steps. Read the steps for your OS below.

### Windows

On Windows you most likely installed the driver that came with the original app.
This driver will claim the device when you plug in the USB cable. Hence the
browser will not be able to access the device. You need to change the driver
with more generic WinUSB driver. When you do this, the COM port will not appear
in the original Windows software anymore. To return the old behavior, you can
always install the original manufacturer driver using the software bundled
with the Windows app.

To change the driver on Windows:

1. Download [Zadig](https://zadig.akeo.ie/) and run it.
2. In the menu, click `Options` and ensure `List All Devices` is checked.
3. From the dropdown select `USB Serial`.
4. On the right `Target Driver`, ensure WinUSB is selected, see the screenshot below.
5. Click `Replace Driver`.

![Zadig Windows](images/zadig-windows.png)

Unplug and plug the USB cable back to the machine. You should now be able to
connect to the device using WebUSB.

### Linux

When you plug in the USB cable, Linux `ch341` driver will claim the USB device
and you cannot connect to it using WebUSB anymore. You need to unbind it first.
You can do one time unbind with following command. This will work until you plug
in the USB cable again.

```bash
sudo sh -c 'echo "1-2.3:1.0" > /sys/bus/usb/drivers/ch341/unbind'
```

If you want to automatically unbind this when the cable is plugged in, you can
add the following udev rule

```bash
sudo tee /etc/udev/rules.d/99-ebc-tester.rules << 'EOF'
# Allow browser access to EBC battery tester (CH340, vid:1a86 pid:7523)
SUBSYSTEM=="usb", ATTRS{idVendor}=="1a86", ATTRS{idProduct}=="7523", MODE="0664", TAG+="uaccess"

# Release ch341 kernel driver immediately after it binds, so WebUSB can claim the interface
ACTION=="bind", SUBSYSTEM=="usb", ATTRS{idVendor}=="1a86", ATTRS{idProduct}=="7523", DRIVER=="ch341", RUN+="/bin/sh -c 'echo %k > /sys/bus/usb/drivers/ch341/unbind'"
EOF
```

Then reload the udev rules with

```bash
sudo udevadm control --reload-rules
```

Now you should be able to connect to the serial port with WebUSB.

To restore the original behavior, just remove the udev rule added above.

## Development

### Native Target Locally

To develop the app locally with auto compile, run

```bash
cargo-watch -x run
```

### Web Locally

You can compile your app to [WASM](https://en.wikipedia.org/wiki/WebAssembly)
and publish it as a web page.

We use [Trunk](https://trunkrs.dev/) to build for web target.

1. Install the required target with `rustup target add wasm32-unknown-unknown`.
2. Install Trunk with `cargo install --locked trunk`.
3. Run `trunk serve` to build and serve on `http://127.0.0.1:8080`. Trunk will rebuild automatically if you edit the project.
4. Open `http://127.0.0.1:8080/index.html#dev` in a browser. See the warning below.

> `assets/sw.js` script will try to cache our app, and loads the cached version when it cannot connect to server allowing your app to work offline (like PWA).
> appending `#dev` to `index.html` will skip this caching, allowing us to load the latest builds during development.

### VSCode WASM Target

By default, VSCode and rust-analyzer compile and show errors for the native
target. To get errors and code completion for the WASM target instead, uncomment
the following line in [.vscode/settings.json](.vscode/settings.json):

```json
"rust-analyzer.cargo.target": "wasm32-unknown-unknown",
```

Remember to revert this when switching back to native development.

### Creating a Release

1. Bump the version in `Cargo.toml`.
2. Commit and push to main, then wait for CI to pass.
3. Tag the commit with the matching version and push the tag:

```bash
git tag v0.2.0
git push --tags
```

The release workflow will validate that the tag matches the version in `Cargo.toml`,
build Linux, Windows and WASM targets, and publish a GitHub release with all three
as downloadable assets.

### CI Checks

To run all CI checks locally at once, use the provided script

```bash
./check.sh
```

This runs cargo check, formatting, Clippy, tests and a Trunk WASM build.

### Rustfmt

This project uses rustfmt for code formatting. There is also a CI check that
enforces correct formatting. Run the formatter locally with

```bash
cargo fmt
```

To only check without modifying files, run

```bash
cargo fmt -- --check
```

### Clippy

This project uses Clippy linter. There is also a CI check that fails on any
warnings. Run Clippy locally with

```bash
cargo clippy --target wasm32-unknown-unknown
```

## Important Resources

* Documentation of the communication protocol used:
  https://pop.fsck.pl/hardware/zketech-ebc-a20.html.
* Python tool to control EBC device from the command-line:
  https://gist.github.com/enkiusz/6408645efd622b8a638a14957cd37f47.
* Example code how to configure CH340 serial chip with WebUSB with correct
  settings like baud rate and other things:
  https://github.com/selevo/WebUsbSerialTerminal/blob/main/serial.js
