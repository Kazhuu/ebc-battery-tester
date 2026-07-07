use crate::device::{OUTBOUND_FRAME_SIZE, OutboundFrame};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

pub(super) struct UsbState {
    pub(super) device: web_sys::UsbDevice,
    pub(super) out_endpoint_num: u8,
    pub(super) in_endpoint_num: u8,
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
        device
            .control_transfer_out_with_u8_slice(
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
    send(0xA4, 0x00DF, 0x0000)?.await?; // modem ctrl: DTR + RTS on
    send(0xA4, 0x009F, 0x0000)?.await?; // modem ctrl: call mode
    send(0x9A, 0x2727, 0x0000)?.await?; // reset control status
    send(0x9A, 0x1312, 0xB282)?.await?; // baud factor: 9600
    send(0x9A, 0x0F2C, 0x0008)?.await?; // baud offset: 9600
    send(0x9A, 0x2518, 0x00CB)?.await?; // line control: 8 bit | odd parity | 1 stop
    send(0x9A, 0x2727, 0x0000)?.await?; // control status
    send(0x9A, 0x1312, 0xB282)?.await?; // baud factor: 9600 (final set)
    send(0x9A, 0x0F2C, 0x0008)?.await?; // baud offset: 9600 (final set)
    send(0x9A, 0x2727, 0x0000)?.await?; // control status (final)

    Ok(())
}

pub(super) async fn connect(device_index: usize) -> Result<UsbState, String> {
    log::info!("Connecting to device at index {device_index}...");
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
    let (Some(interface_num), Some(out_endpoint_num), Some(in_endpoint_num)) =
        (interface_num, out_endpoint_num, in_endpoint_num)
    else {
        return Err("No bulk endpoints found on USB device".to_owned());
    };
    log::info!(
        "Using interface {interface_num}, OUT endpoint {out_endpoint_num}, IN endpoint {in_endpoint_num}"
    );

    JsFuture::from(device.claim_interface(interface_num))
        .await
        .map_err(|e| format!("Failed to claim interface {interface_num}: {e:?}"))?;

    let mut bytes: [u8; OUTBOUND_FRAME_SIZE] = OutboundFrame::Connect(device_index).into();
    let promise = device
        .transfer_out_with_u8_slice(out_endpoint_num, &mut bytes)
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

pub(super) async fn disconnect(
    device: &web_sys::UsbDevice,
    out_endpoint_num: u8,
) -> Result<(), JsValue> {
    log::info!("Disconnecting from device...");
    let mut bytes: [u8; OUTBOUND_FRAME_SIZE] = OutboundFrame::Disconnect.into();
    let promise = device
        .transfer_out_with_u8_slice(out_endpoint_num, &mut bytes)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Disconnect command failed: {e:?}"))?;
    JsFuture::from(device.close()).await?;
    Ok(())
}

pub(super) async fn stop(device: &web_sys::UsbDevice, out_endpoint_num: u8) -> Result<(), JsValue> {
    let mut bytes: [u8; OUTBOUND_FRAME_SIZE] = OutboundFrame::Stop.into();
    let promise = device
        .transfer_out_with_u8_slice(out_endpoint_num, &mut bytes)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e: JsValue| format!("Stop command failed: {e:?}"))?;
    Ok(())
}
