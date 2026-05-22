use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use crate::device::{ConnectionStatus, DeviceCommand, DeviceEvent, UsbDeviceInfo};
use futures::channel::mpsc::{UnboundedSender, UnboundedReceiver};
use futures::channel::oneshot;
use futures::StreamExt as _;
use futures::FutureExt as _;

const VENDOR_ID: u16 = 0x1A86;

struct UsbState {
    device: web_sys::UsbDevice,
    out_endpoint_num: u8,
    in_endpoint_num: u8,
}

pub fn enumerate_devices(event_tx: UnboundedSender<DeviceEvent>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let usb = window.navigator().usb();
        match JsFuture::from(usb.get_devices()).await {
            Ok(value) => {
                let mut devices = Vec::new();
                for item in js_sys::Array::from(&value) {
                    let device: web_sys::UsbDevice = item.unchecked_into();
                    devices.push(UsbDeviceInfo {
                        product_name: device.product_name().unwrap_or_default(),
                        manufacturer_name: device.manufacturer_name().unwrap_or_default(),
                        vendor_id: device.vendor_id(),
                        product_id: device.product_id(),
                    });
                }
                event_tx.unbounded_send(DeviceEvent::DevicesUpdated(devices)).ok();
                ctx.request_repaint();
            }
            Err(e) => {
                log::error!("Failed to enumerate USB devices: {e:?}");
            }
        }
    });
}

pub fn request_device(event_tx: UnboundedSender<DeviceEvent>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let usb = window.navigator().usb();
        let filter = web_sys::UsbDeviceFilter::new();
        filter.set_vendor_id(VENDOR_ID);
        let options = web_sys::UsbDeviceRequestOptions::new(&[filter]);
        match JsFuture::from(usb.request_device(&options)).await {
            Ok(_) => enumerate_devices(event_tx, ctx),
            Err(e) => {
                log::warn!("USB device request cancelled or denied: {e:?}");
            }
        }
    });
}

pub fn spawn_device_task(
    ctx: egui::Context,
    cmd_rx: UnboundedReceiver<DeviceCommand>,
    event_tx: UnboundedSender<DeviceEvent>,
) {
    wasm_bindgen_futures::spawn_local(device_task(ctx, cmd_rx, event_tx));
}

async fn device_task(
    ctx: egui::Context,
    mut cmd_rx: UnboundedReceiver<DeviceCommand>,
    event_tx: UnboundedSender<DeviceEvent>,
) {
    let mut stop_reading_tx: Option<oneshot::Sender<()>> = None;
    let mut device: Option<web_sys::UsbDevice> = None;
    let mut out_endpoint_num: Option<u8> = None;
    loop {
        match cmd_rx.next().await {
            Some(DeviceCommand::Connect(idx)) => {
                event_tx.unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Connecting)).ok();
                ctx.request_repaint();
                match connect(idx).await {
                    Ok(state) => {
                        let dev = state.device;
                        out_endpoint_num = Some(state.out_endpoint_num);
                        let (stop_tx, stop_rx) = oneshot::channel();
                        stop_reading_tx = Some(stop_tx);
                        event_tx.unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Connected)).ok();
                        wasm_bindgen_futures::spawn_local(reading_task(
                            dev.clone(),
                            state.in_endpoint_num,
                            event_tx.clone(),
                            stop_rx,
                            ctx.clone(),
                        ));
                        device = Some(dev);
                        ctx.request_repaint();
                    }
                    Err(e) => {
                        log::error!("Failed to connect: {e}");
                        event_tx.unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Error(e))).ok();
                        ctx.request_repaint();
                    }
                }
            }
            Some(DeviceCommand::Disconnect) => {
                if let (Some(device), Some(ep)) = (&device, out_endpoint_num) {
                    if let Some(stop_tx) = stop_reading_tx.take() {
                        let _ = stop_tx.send(());
                    }
                    let _ = disconnect(device, ep).await;

                }
                device = None;
                out_endpoint_num = None;
                event_tx.unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Disconnected)).ok();
                ctx.request_repaint();
            }
            None => break,
        }
    }
}

async fn reading_task(
    device: web_sys::UsbDevice,
    in_endpoint: u8,
    event_tx: UnboundedSender<DeviceEvent>,
    mut stop_reading_rx: oneshot::Receiver<()>,
    ctx: egui::Context,
) {
    loop {
        let transfer = JsFuture::from(device.transfer_in(in_endpoint, 30)).fuse();
        futures::pin_mut!(transfer);
        futures::select! {
            result = transfer => match result {
                Ok(value) => {
                    let result: web_sys::UsbInTransferResult = value.unchecked_into();
                    if let Some(data) = result.data() {
                        let bytes = js_sys::Uint8Array::new(&data.buffer()).to_vec();
                        if !bytes.is_empty() {
                            event_tx.unbounded_send(DeviceEvent::Frame(bytes)).ok();
                            ctx.request_repaint();
                        }
                    }
                }
                Err(e) => {
                    log::error!("Bulk IN transfer failed: {e:?}");
                    event_tx.unbounded_send(DeviceEvent::StatusChanged(
                        ConnectionStatus::Error("Read error: connection lost".to_owned()),
                    )).ok();
                    ctx.request_repaint();
                    return;
                }
            },
            _ = stop_reading_rx => return,
        }
    }
}

// Configures the CH340 USB-serial chip for 9600 baud, 8 data bits, odd parity, 1 stop bit.
// Sequence derived from https://github.com/selevo/WebUsbSerialTerminal/blob/main/serial.js
// Every CH340 control write carries a 1-byte null payload alongside the register/value in
// the SETUP packet wValue/wIndex fields.
async fn ch340_configure(device: &web_sys::UsbDevice) -> Result<(), JsValue> {
    // Each call sends one vendor control OUT transfer with a 1-byte null data stage.
    // request = CH340 vendor command, value = register address, index = register value.
    let send = |request: u8, value: u16, index: u16| {
        let mut data = [0u8; 1];
        device.control_transfer_out_with_u8_slice(
            &web_sys::UsbControlTransferParameters::new(
                index,
                web_sys::UsbRecipient::Device,
                request,
                web_sys::UsbRequestType::Vendor,
                value,
            ),
            &mut data,
        )
        .map(JsFuture::from)
    };

    // The CH340 requires a specific sequence of control transfers to initialize the serial port.
    send(0xA1, 0xC29C, 0xB2B9)?.await?; // serial init
    send(0xA4, 0x00DF, 0x0000)?.await?;  // modem ctrl: DTR + RTS on
    send(0xA4, 0x009F, 0x0000)?.await?;  // modem ctrl: call mode
    send(0x9A, 0x2727, 0x0000)?.await?;  // reset control status
    send(0x9A, 0x1312, 0xB282)?.await?;  // baud factor: 9600
    send(0x9A, 0x0F2C, 0x0008)?.await?;  // baud offset: 9600
    send(0x9A, 0x2518, 0x00CB)?.await?;  // line control: 8 bit | odd parity | 1 stop
    send(0x9A, 0x2727, 0x0000)?.await?;  // control status
    send(0x9A, 0x1312, 0xB282)?.await?;  // baud factor: 9600 (final set)
    send(0x9A, 0x0F2C, 0x0008)?.await?;  // baud offset: 9600 (final set)
    send(0x9A, 0x2727, 0x0000)?.await?;  // control status (final)

    Ok(())
}

async fn connect(device_index: usize) -> Result<UsbState, String> {
    let window = web_sys::window().ok_or("No window context")?;
    let usb = window.navigator().usb();

    let value = JsFuture::from(usb.get_devices())
        .await
        .map_err(|e| format!("Failed to get USB devices: {e:?}"))?;
    let item = js_sys::Array::from(&value).get(device_index as u32);
    if item.is_undefined() {
        return Err(format!("No USB device at index {device_index}"));
    }
    let device: web_sys::UsbDevice = item.unchecked_into();

    JsFuture::from(device.open())
        .await
        .map_err(|e| format!("Failed to open device: {e:?}"))?;
    JsFuture::from(device.select_configuration(1))
        .await
        .map_err(|e| format!("Failed to select configuration: {e:?}"))?;
    ch340_configure(&device)
        .await
        .map_err(|e| format!("Failed to configure CH340: {e:?}"))?;

    // Find bulk IN and OUT endpoints from the active configuration.
    let config = device
        .configuration()
        .ok_or("USB device has no active configuration")?;
    let mut interface_num: Option<u8> = None;
    let mut out_endpoint_num: Option<u8> = None;
    let mut in_endpoint_num: Option<u8> = None;
    for iface_val in config.interfaces() {
        let iface: web_sys::UsbInterface = iface_val.unchecked_into();
        let interface_number = iface.interface_number();
        for ep_val in iface.alternate().endpoints() {
            let ep: web_sys::UsbEndpoint = ep_val.unchecked_into();
            if ep.type_() == web_sys::UsbEndpointType::Bulk {
                if ep.direction() == web_sys::UsbDirection::Out {
                    interface_num = Some(interface_number);
                    out_endpoint_num = Some(ep.endpoint_number());
                } else if ep.direction() == web_sys::UsbDirection::In {
                    in_endpoint_num = Some(ep.endpoint_number());
                }
            }
        }
    }
    let (Some(interface_num), Some(out_endpoint_num), Some(in_endpoint_num)) = (
        interface_num,
        out_endpoint_num,
        in_endpoint_num,
    ) else {
        return Err("No bulk endpoints found on USB device".to_owned());
    };
    log::info!("Using interface {interface_num}, OUT endpoint {out_endpoint_num}, IN endpoint {in_endpoint_num}");

    JsFuture::from(device.claim_interface(interface_num))
        .await
        .map_err(|e| format!("Failed to claim interface {interface_num}: {e:?}"))?;

    // Send connect command to the device. This will display '-PC-' on the LCD screen.
    let mut command = [0xfa_u8, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0xf8];
    let promise = device
        .transfer_out_with_u8_slice(out_endpoint_num, &mut command)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Connect command failed: {e:?}"))?;

    Ok(UsbState {
        device,
        out_endpoint_num,
        in_endpoint_num,
    })
}

async fn disconnect(device: &web_sys::UsbDevice, out_endpoint_num: u8) -> Result<(), JsValue> {
    // Send disconnect command to the device. After this '-PC-' disappears from LCD screen.
    let mut command = [0xfa_u8, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0xf8];
    let promise = device
        .transfer_out_with_u8_slice(out_endpoint_num, &mut command)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Disconnect command failed: {e:?}"))?;
    JsFuture::from(device.close()).await?;
    Ok(())
}
