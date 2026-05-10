use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbDeviceInfo {
    pub product_name: String,
    pub manufacturer_name: String,
    pub vendor_id: u16,
    pub product_id: u16,
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

pub fn enumerate_devices(devices: Rc<RefCell<Vec<UsbDeviceInfo>>>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let usb = window.navigator().usb();
        match JsFuture::from(usb.get_devices()).await {
            Ok(value) => {
                let mut list = devices.borrow_mut();
                list.clear();
                for item in js_sys::Array::from(&value) {
                    let device: web_sys::UsbDevice = item.unchecked_into();
                    list.push(UsbDeviceInfo {
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

pub fn request_device(devices: Rc<RefCell<Vec<UsbDeviceInfo>>>, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let usb = window.navigator().usb();
        let mut filter = web_sys::UsbDeviceFilter::new();
        filter.vendor_id(0x1A86);
        let options = web_sys::UsbDeviceRequestOptions::new(&[filter]);
        match JsFuture::from(usb.request_device(&options)).await {
            Ok(_) => enumerate_devices(devices, ctx),
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

pub fn connect_and_write(device_index: usize, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let usb = window.navigator().usb();
        let Ok(value) = JsFuture::from(usb.get_devices()).await else {
            log::error!("Failed to get USB devices");
            return;
        };

        let Ok(index_u32) = u32::try_from(device_index) else {
            log::error!("Device index out of range");
            return;
        };
        let item = js_sys::Array::from(&value).get(index_u32);
        if item.is_undefined() {
            log::error!("No USB device at index {device_index}");
            return;
        }
        let device: web_sys::UsbDevice = item.unchecked_into();

        if let Err(e) = JsFuture::from(device.open()).await {
            log::error!("Failed to open USB device: {e:?}");
            return;
        }
        if let Err(e) = JsFuture::from(device.select_configuration(1)).await {
            log::error!("Failed to select USB configuration: {e:?}");
            return;
        }
        if let Err(e) = ch340_configure(&device).await {
            log::error!("Failed to configure CH340: {e:?}");
            return;
        }
        // Find the interface and bulk OUT endpoint from the active configuration.
        let Some(config) = device.configuration() else {
            log::error!("USB device has no active configuration");
            return;
        };
        let mut found_interface: Option<u8> = None;
        let mut found_endpoint: Option<u8> = None;
        'search: for iface_val in config.interfaces() {
            let iface: web_sys::UsbInterface = iface_val.unchecked_into();
            for ep_val in iface.alternate().endpoints() {
                let ep: web_sys::UsbEndpoint = ep_val.unchecked_into();
                if ep.direction() == web_sys::UsbDirection::Out
                    && ep.type_() == web_sys::UsbEndpointType::Bulk
                {
                    found_interface = Some(iface.interface_number());
                    found_endpoint = Some(ep.endpoint_number());
                    break 'search;
                }
            }
        }
        let (Some(interface_num), Some(endpoint_num)) = (found_interface, found_endpoint) else {
            log::error!("No bulk OUT endpoint found on USB device");
            return;
        };
        log::info!("Using interface {interface_num}, endpoint {endpoint_num}");

        if let Err(e) = JsFuture::from(device.claim_interface(interface_num)).await {
            log::error!("Failed to claim USB interface {interface_num}: {e:?}");
            return;
        }

        // Send connect command to the device. This will display '-PC-' on the device screen.
        let mut command = [0xfa_u8, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0xf8];
        match device.transfer_out_with_u8_slice(endpoint_num, &mut command) {
            Ok(promise) => match JsFuture::from(promise).await {
                Ok(_) => log::info!("Command sent to USB device"),
                Err(e) => log::error!("Transfer failed: {e:?}"),
            },
            Err(e) => log::error!("Failed to start transfer: {e:?}"),
        }

        ctx.request_repaint();
    });
}
