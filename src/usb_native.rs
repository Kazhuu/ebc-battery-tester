use std::io::{Read as _, Write as _};

use crate::device::{ConnectionStatus, DeviceEvent, OutboundFrame, UsbDeviceInfo};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use serialport::SerialPortType;

const VENDOR_ID: u16 = 0x1A86;

#[expect(clippy::needless_pass_by_value)]
pub fn enumerate_devices(event_tx: UnboundedSender<DeviceEvent>) {
    let all_ports = serialport::available_ports().unwrap_or_default();
    log::debug!("All serial ports: {all_ports:?}");
    let devices = all_ports
        .into_iter()
        .filter_map(|p| {
            if let SerialPortType::UsbPort(usb) = p.port_type
                && usb.vid == VENDOR_ID
            {
                return Some(UsbDeviceInfo {
                    product_name: usb.product.unwrap_or_default(),
                    manufacturer_name: usb.manufacturer.unwrap_or_default(),
                    vendor_id: usb.vid,
                    product_id: usb.pid,
                });
            }
            None
        })
        .collect();
    event_tx
        .unbounded_send(DeviceEvent::DevicesUpdated(devices))
        .ok();
}

pub fn spawn_device_task(
    ctx: egui::Context,
    cmd_rx: UnboundedReceiver<OutboundFrame>,
    event_tx: UnboundedSender<DeviceEvent>,
) {
    std::thread::spawn(move || device_thread(ctx, cmd_rx, event_tx));
}

fn find_ch340_port(idx: usize) -> Option<String> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .filter(|p| matches!(&p.port_type, SerialPortType::UsbPort(usb) if usb.vid == VENDOR_ID))
        .nth(idx)
        .map(|p| p.port_name)
}

fn connect(idx: usize) -> Result<Box<dyn serialport::SerialPort>, String> {
    let name = find_ch340_port(idx).ok_or_else(|| format!("No CH340 device at index {idx}"))?;
    let mut port = serialport::new(&name, 9600)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::Odd)
        .stop_bits(serialport::StopBits::One)
        .timeout(std::time::Duration::from_millis(10))
        .open()
        .map_err(|e| format!("Failed to open {name}: {e}"))?;
    let bytes: [u8; 10] = OutboundFrame::Connect(idx).into();
    port.write_all(&bytes)
        .map_err(|e| format!("Failed to send connect command: {e}"))?;
    Ok(port)
}

#[expect(clippy::needless_pass_by_value)]
fn device_thread(
    ctx: egui::Context,
    mut cmd_rx: UnboundedReceiver<OutboundFrame>,
    event_tx: UnboundedSender<DeviceEvent>,
) {
    let mut port: Option<Box<dyn serialport::SerialPort>> = None;
    let mut buf: Vec<u8> = Vec::new();

    loop {
        loop {
            match cmd_rx.try_recv() {
                Ok(OutboundFrame::Connect(idx)) => {
                    event_tx
                        .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Connecting))
                        .ok();
                    ctx.request_repaint();
                    match connect(idx) {
                        Ok(p) => {
                            port = Some(p);
                            event_tx
                                .unbounded_send(DeviceEvent::StatusChanged(
                                    ConnectionStatus::Connected,
                                ))
                                .ok();
                        }
                        Err(e) => {
                            log::error!("Failed to connect: {e}");
                            event_tx
                                .unbounded_send(DeviceEvent::StatusChanged(
                                    ConnectionStatus::Error(e),
                                ))
                                .ok();
                        }
                    }
                    ctx.request_repaint();
                }
                Ok(OutboundFrame::Disconnect) => {
                    if let Some(ref mut p) = port {
                        let bytes: [u8; 10] = OutboundFrame::Disconnect.into();
                        if let Err(e) = p.write_all(&bytes) {
                            log::error!("Failed to send disconnect frame: {e}");
                        }
                    }
                    port = None;
                    buf.clear();
                    event_tx
                        .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Disconnected))
                        .ok();
                    ctx.request_repaint();
                }
                Ok(frame) => {
                    if let Some(ref mut p) = port {
                        let bytes: [u8; 10] = frame.into();
                        if let Err(e) = p.write_all(&bytes) {
                            log::error!("Failed to send frame: {e}");
                        }
                    }
                }
                Err(_) => break,
            }
        }

        if let Some(ref mut p) = port {
            let mut tmp = [0u8; 64];
            match p.read(&mut tmp) {
                Ok(n) if n > 0 => {
                    buf.extend_from_slice(&tmp[..n]);
                    for frame in crate::device::process_buffer(&mut buf) {
                        event_tx.unbounded_send(DeviceEvent::Frame(frame)).ok();
                        ctx.request_repaint();
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    log::error!("Serial read error: {e}");
                    port = None;
                    buf.clear();
                    event_tx
                        .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Error(
                            "Read error: connection lost".to_owned(),
                        )))
                        .ok();
                    ctx.request_repaint();
                }
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
