# EBC-A20 Battery Tester

This project is WIP!

Cross-platform desktop and browser application to control ZTE Tech EBC-A20
battery tester.

The deployed web app is available at
[mauri.codes/ebc-battery-tester](https://mauri.codes/ebc-battery-tester).

The app is built with Rust using egui and eframe.

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

### Testing locally

`cargo run --release`

### Web Locally

You can compile your app to [WASM](https://en.wikipedia.org/wiki/WebAssembly) and publish it as a web page.

We use [Trunk](https://trunkrs.dev/) to build for web target.

1. Install the required target with `rustup target add wasm32-unknown-unknown`.
2. Install Trunk with `cargo install --locked trunk`.
3. Run `trunk serve` to build and serve on `http://127.0.0.1:8080`. Trunk will rebuild automatically if you edit the project.
4. Open `http://127.0.0.1:8080/index.html#dev` in a browser. See the warning below.

> `assets/sw.js` script will try to cache our app, and loads the cached version when it cannot connect to server allowing your app to work offline (like PWA).
> appending `#dev` to `index.html` will skip this caching, allowing us to load the latest builds during development.

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
