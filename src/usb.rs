use std::collections::VecDeque;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use std::error::Error;

#[derive(Clone, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbDeviceInfo {
    pub product_name: String,
    pub manufacturer_name: String,
    pub vendor_id: u16,
    pub product_id: u16,
}

pub struct UsbState {
    pub available_devices: Vec<UsbDeviceInfo>,
    pub selected_index: Option<usize>,
    pub status: ConnectionStatus,
    pub device : Option<web_sys::UsbDevice>,
    pub interface_num: Option<u8>,
    pub endpoint_num: Option<u8>,
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
            endpoint_num: None,
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

async fn connect_inner(device_index: usize, usb_state: Rc<RefCell<UsbState>>) -> Result<web_sys::UsbDevice, String> {
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

    // Find the interface and bulk OUT endpoint from the active configuration.
    let config = device
        .configuration()
        .ok_or("USB device has no active configuration")?;
    'search: for iface_val in config.interfaces() {
        let iface: web_sys::UsbInterface = iface_val.unchecked_into();
        for ep_val in iface.alternate().endpoints() {
            let ep: web_sys::UsbEndpoint = ep_val.unchecked_into();
            if ep.direction() == web_sys::UsbDirection::Out
                && ep.type_() == web_sys::UsbEndpointType::Bulk
            {
                usb_state.borrow_mut().interface_num = Some(iface.interface_number());
                usb_state.borrow_mut().endpoint_num = Some(ep.endpoint_number());
                break 'search;
            }
        }
    }
    let (Some(interface_num), Some(endpoint_num)) = (usb_state.borrow().interface_num, usb_state.borrow().endpoint_num) else {
        return Err("No bulk OUT endpoint found on USB device".to_owned());
    };
    log::info!("Using interface {interface_num}, endpoint {endpoint_num}");

    JsFuture::from(device.claim_interface(interface_num))
        .await
        .map_err(|e| format!("Failed to claim interface {interface_num}: {e:?}"))?;

    // Send connect command to the device. This will display '-PC-' on the LCD screen.
    let mut command = [0xfa_u8, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0xf8];
    let promise = device
        .transfer_out_with_u8_slice(endpoint_num, &mut command)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Connect command failed: {e:?}"))?;

    Ok(device)
}

async fn inner_disconnect(usb_state: Rc<RefCell<UsbState>>) -> Result<(), JsValue> {
    // Send disconnect command to the device. After this '-PC-' disappears from LCD screen.
    let mut command = [0xfa_u8, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0xf8];
    let device = usb_state.borrow().device.clone().ok_or("No device connected")?;
    let endpoint_num = usb_state.borrow().endpoint_num.ok_or("No endpoint found")?;
    let promise = device
        .transfer_out_with_u8_slice(endpoint_num, &mut command)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Connect command failed: {e:?}"))?;
    JsFuture::from(device.close()).await?;
    Ok(())
}

pub fn connect(device_index: usize, usb_state: Rc<RefCell<UsbState>>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        usb_state.borrow_mut().status = ConnectionStatus::Connecting;
        ctx.request_repaint();

        match connect_inner(device_index, Rc::clone(&usb_state)).await {
            Ok(device) => {
                log::info!("Connected to USB device");
                let mut state = usb_state.borrow_mut();
                state.device = Some(device);
                state.status = ConnectionStatus::Connected;
            }
            Err(e) => {
                log::error!("Failed to connect: {e}");
                usb_state.borrow_mut().status = ConnectionStatus::Error(e);
            }
        }

        ctx.request_repaint();
    });
}

pub fn disconnect(usb_state: Rc<RefCell<UsbState>>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(_) = &usb_state.borrow().device {
            let _ = inner_disconnect(Rc::clone(&usb_state)).await;
        }
        usb_state.borrow_mut().device = None;
        usb_state.borrow_mut().status = ConnectionStatus::Disconnected;
        ctx.request_repaint();
    });
}
