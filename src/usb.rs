use std::collections::VecDeque;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use crate::device::{ConnectionStatus, DeviceCommand, DeviceEvent};
use futures::channel::mpsc::{UnboundedSender, UnboundedReceiver};
use futures::channel::oneshot;
use futures::StreamExt as _;
use futures::FutureExt as _;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbDeviceInfo {
    pub product_name: String,
    pub manufacturer_name: String,
    pub vendor_id: u16,
    pub product_id: u16,
}

// TODO: Remove extra fields and clean this up to elsewhere.
pub struct UsbState {
    pub available_devices: Vec<UsbDeviceInfo>,
    pub selected_index: Option<usize>,
    pub status: ConnectionStatus,
    pub device: Option<web_sys::UsbDevice>,
    pub interface_num: Option<u8>,
    pub out_endpoint_num: Option<u8>,
    pub in_endpoint_num: Option<u8>,
    pub rx_frames: VecDeque<Vec<u8>>,
}

impl Default for UsbState {
    fn default() -> Self {
        Self {
            available_devices: Vec::new(),
            selected_index: None,
            status: ConnectionStatus::Disconnected,
            device: None,
            interface_num: None,
            out_endpoint_num: None,
            in_endpoint_num: None,
            rx_frames: VecDeque::new(),
        }
    }
}

impl std::fmt::Display for UsbDeviceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.product_name.is_empty() {
            write!(f, "Unknown ({:04x}:{:04x})", self.vendor_id, self.product_id)
        } else {
            write!(
                f,
                "{} ({:04x}:{:04x})",
                self.product_name, self.vendor_id, self.product_id
            )
        }
    }
}

// TODO: Make this return better type.
pub fn enumerate_devices(usb_state: Rc<RefCell<UsbState>>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let usb = window.navigator().usb();
        match JsFuture::from(usb.get_devices()).await {
            Ok(value) => {
                let mut state = usb_state.borrow_mut();
                state.available_devices.clear();
                for item in js_sys::Array::from(&value) {
                    let device: web_sys::UsbDevice = item.unchecked_into();
                    state.available_devices.push(UsbDeviceInfo {
                        product_name: device.product_name().unwrap_or_default(),
                        manufacturer_name: device.manufacturer_name().unwrap_or_default(),
                        vendor_id: device.vendor_id(),
                        product_id: device.product_id(),
                    });
                }
                ctx.request_repaint();
            }
            Err(e) => {
                log::error!("Failed to enumerate USB devices: {e:?}");
            }
        }
    });
}

// TODO: Make this return better type and not use the usb state.
pub fn request_device(usb_state: Rc<RefCell<UsbState>>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let usb = window.navigator().usb();
        let filter = web_sys::UsbDeviceFilter::new();
        filter.set_vendor_id(0x1A86);
        let options = web_sys::UsbDeviceRequestOptions::new(&[filter]);
        match JsFuture::from(usb.request_device(&options)).await {
            Ok(_) => enumerate_devices(usb_state, ctx),
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
    let mut usb_state = UsbState::default();
    for iface_val in config.interfaces() {
        let iface: web_sys::UsbInterface = iface_val.unchecked_into();
        let interface_number = iface.interface_number();
        for ep_val in iface.alternate().endpoints() {
            let ep: web_sys::UsbEndpoint = ep_val.unchecked_into();
            if ep.type_() == web_sys::UsbEndpointType::Bulk {
                if ep.direction() == web_sys::UsbDirection::Out {
                    usb_state.interface_num = Some(interface_number);
                    usb_state.out_endpoint_num = Some(ep.endpoint_number());
                } else if ep.direction() == web_sys::UsbDirection::In {
                    usb_state.in_endpoint_num = Some(ep.endpoint_number());
                }
            }
        }
    }
    let (Some(interface_num), Some(out_endpoint_num), Some(in_endpoint_num)) = (
        usb_state.interface_num,
        usb_state.out_endpoint_num,
        usb_state.in_endpoint_num,
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

    usb_state.device = Some(device);
    Ok(usb_state)
}

async fn disconnect(device: &web_sys::UsbDevice, out_endpoint_num: u8) -> Result<(), JsValue> {
    // Send disconnect command to the device. After this '-PC-' disappears from LCD screen.
    let mut command = [0xfa_u8, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0xf8];
    let promise = device
        .transfer_out_with_u8_slice(out_endpoint_num, &mut command)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Connect command failed: {e:?}"))?;
    JsFuture::from(device.close()).await?;
    Ok(())
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
                        let dev = state.device.unwrap();
                        out_endpoint_num = state.out_endpoint_num;
                        let (stop_tx, stop_rx) = oneshot::channel();
                        stop_reading_tx = Some(stop_tx);
                        event_tx.unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Connected)).ok();
                        wasm_bindgen_futures::spawn_local(reading_task(
                            dev.clone(),
                            state.in_endpoint_num.unwrap(),
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
                if device.is_some() && out_endpoint_num.is_some() {
                    if let Some(stop_tx) = stop_reading_tx.take() {
                        let _ = stop_tx.send(());
                    }
                    let _ = disconnect(&device.as_ref().unwrap(), out_endpoint_num.unwrap()).await;
                    device = None;
                    out_endpoint_num = None;
                }
                event_tx.unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Disconnected)).ok();
                ctx.request_repaint();
            }
            None => break,
        }
    }
}
