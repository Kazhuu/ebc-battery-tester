use std::{cell::RefCell, rc::Rc};

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
        match wasm_bindgen_futures::JsFuture::from(usb.get_devices()).await {
            Ok(array) => {
                let mut list = devices.borrow_mut();
                list.clear();
                for device in array {
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
        let options = web_sys::UsbDeviceRequestOptions::new(&[]);
        match wasm_bindgen_futures::JsFuture::from(usb.request_device(&options)).await {
            Ok(_) => enumerate_devices(devices, ctx),
            Err(e) => {
                log::warn!("USB device request cancelled or denied: {e:?}");
            }
        }
    });
}
