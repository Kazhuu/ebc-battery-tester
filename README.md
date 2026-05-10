# EBC-A20 Battery Tester

This project is WIP!

Cross-platform desktop and browser application to control ZTE Tech EBC-A20
battery tester.

Built with egui and eframe and is based on the
https://github.com/JOGAsoft/EBC-controller.

## Testing locally

`cargo run --release`

## Web Locally

You can compile your app to [WASM](https://en.wikipedia.org/wiki/WebAssembly) and publish it as a web page.

We use [Trunk](https://trunkrs.dev/) to build for web target.
1. Install the required target with `rustup target add wasm32-unknown-unknown`.
2. Install Trunk with `cargo install --locked trunk`.
3. Run `trunk serve` to build and serve on `http://127.0.0.1:8080`. Trunk will rebuild automatically if you edit the project.
4. Open `http://127.0.0.1:8080/index.html#dev` in a browser. See the warning below.

> `assets/sw.js` script will try to cache our app, and loads the cached version when it cannot connect to server allowing your app to work offline (like PWA).
> appending `#dev` to `index.html` will skip this caching, allowing us to load the latest builds during development.

## Web Deploy

1. Just run `trunk build --release`.
2. It will generate a `dist` directory as a "static html" website
3. Upload the `dist` directory to any of the numerous free hosting websites including [GitHub Pages](https://docs.github.com/en/free-pro-team@latest/github/working-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site).
4. we already provide a workflow that auto-deploys our app to GitHub pages if you enable it.
> To enable Github Pages, you need to go to Repository -> Settings -> Pages -> Source -> set to `gh-pages` branch and `/` (root).
>
> If `gh-pages` is not available in `Source`, just create and push a branch called `gh-pages` and it should be available.
>
> If you renamed the `main` branch to something else (say you re-initialized the repository with `master` as the initial branch), be sure to edit the github workflows `.github/workflows/pages.yml` file to reflect the change
> ```yml
> on:
>   push:
>     branches:
>       - <branch name>
> ```

## Linux WebUSB udev Rule

When you plug in the USB cable, inux `ch341` driver will claim the USB device
and you cannot connect to it using WebUSB anymore. You need to unbind it first.
You can do one time unbind with following command. This will work until you plug
in the USB cable again.

```bash
sudo sh -c 'echo "1-2.3:1.0" > /sys/bus/usb/drivers/ch341/unbind'
```

If you want to automatically unbind this when the cable is plugged in, you can
add following udev rule

```bash
sudo tee /etc/udev/rules.d/99-ebc-tester.rules << 'EOF'
# Allow browser access to EBC battery tester (CH340, vid:1a86 pid:7523)
SUBSYSTEM=="usb", ATTRS{idVendor}=="1a86", ATTRS{idProduct}=="7523", MODE="0664", TAG+="uaccess"

# Release ch341 kernel driver immediately after it binds, so WebUSB can claim the interface
ACTION=="bind", SUBSYSTEM=="usb", ATTRS{idVendor}=="1a86", ATTRS{idProduct}=="7523", DRIVER=="ch341", RUN+="/bin/sh -c 'echo %k > /sys/bus/usb/drivers/ch341/unbind'"
EOF
sudo udevadm control --reload-rules
```

Then reload the udev rules with

```bash
sudo udevadm control --reload-rules
```

Now you should be to connect to the serial port with WebUSB.

## Important Resources

* Documentation of the communication protocol used:
  https://pop.fsck.pl/hardware/zketech-ebc-a20.html.
* Python tool to control EBC device from the command-line:
  https://gist.github.com/enkiusz/6408645efd622b8a638a14957cd37f47.
* Example code how to configure CH340 serial chip with WebUSB with correct
  settings like baud rate and other things:
  https://github.com/selevo/WebUsbSerialTerminal/blob/main/serial.js
