use std::io::{Read as _, Write as _};
use web_time::Instant;

use crate::{
    device::{ConnectionStatus, DeviceEvent, OUTBOUND_FRAME_SIZE, OutboundFrame, UsbDeviceInfo},
    timer_sync::TimerSync,
};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use serialport::SerialPortType;

const BAUD_RATE: u32 = 9600;
const SLEEP_DURATION: std::time::Duration = std::time::Duration::from_millis(10);

#[expect(clippy::needless_pass_by_value)]
pub fn enumerate_devices(event_tx: UnboundedSender<DeviceEvent>) {
    let all_ports = serialport::available_ports().unwrap_or_default();
    log::debug!("All serial ports: {all_ports:?}");
    let devices = all_ports
        .into_iter()
        .filter_map(|p| {
            if let SerialPortType::UsbPort(usb) = p.port_type
                && usb.vid == crate::device::VENDOR_ID
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

pub fn spawn_device_worker(
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
        .filter(|p| matches!(&p.port_type, SerialPortType::UsbPort(usb) if usb.vid == crate::device::VENDOR_ID))
        .nth(idx)
        .map(|p| p.port_name)
}

fn connect(idx: usize) -> Result<Box<dyn serialport::SerialPort>, String> {
    let name = find_ch340_port(idx).ok_or_else(|| format!("No CH340 device at index {idx}"))?;
    let mut port = serialport::new(&name, BAUD_RATE)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::Odd)
        .stop_bits(serialport::StopBits::One)
        .timeout(std::time::Duration::from_millis(10))
        .open()
        .map_err(|e| format!("Failed to open {name}: {e}"))?;
    let bytes: [u8; OUTBOUND_FRAME_SIZE] = OutboundFrame::Connect(idx).into();
    port.write_all(&bytes)
        .map_err(|e| format!("Failed to send connect command: {e}"))?;
    Ok(port)
}

/// Sends a `TimerSync` frame if the timer sync logic indicates that a new
/// minute boundary has been crossed since the last call.
fn send_time_sync_if_needed(
    time_sync: &mut TimerSync,
    port: &mut Option<Box<dyn serialport::SerialPort>>,
    event_tx: &UnboundedSender<DeviceEvent>,
    ctx: &egui::Context,
) {
    if let Some(frame) = time_sync.check(Instant::now())
        && let Some(p) = port
    {
        let bytes: [u8; OUTBOUND_FRAME_SIZE] = frame.clone().into();
        match p.write_all(&bytes) {
            Ok(()) => {
                event_tx
                    .unbounded_send(DeviceEvent::FrameSent(frame, bytes.to_vec()))
                    .ok();
                ctx.request_repaint();
            }
            Err(e) => {
                log::error!("Failed to send timer sync frame: {e}");
                time_sync.mode_stopped(Instant::now());
            }
        }
    }
}

#[expect(clippy::needless_pass_by_value)]
fn device_thread(
    ctx: egui::Context,
    mut cmd_rx: UnboundedReceiver<OutboundFrame>,
    event_tx: UnboundedSender<DeviceEvent>,
) -> ! {
    let mut port: Option<Box<dyn serialport::SerialPort>> = None;
    let mut buffer: Vec<u8> = Vec::new();
    let mut time_sync = TimerSync::new();

    loop {
        // Process any commands from the UI thread, including connect/disconnect
        // and mode control commands.
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
                            time_sync.mode_stopped(Instant::now());
                        }
                    }
                    ctx.request_repaint();
                }
                Ok(OutboundFrame::Disconnect) => {
                    if let Some(ref mut p) = port {
                        let bytes: [u8; OUTBOUND_FRAME_SIZE] = OutboundFrame::Disconnect.into();
                        if let Err(e) = p.write_all(&bytes) {
                            log::error!("Failed to send disconnect frame: {e}");
                        }
                    }
                    port = None;
                    buffer.clear();
                    event_tx
                        .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Disconnected))
                        .ok();
                    time_sync.mode_stopped(Instant::now());
                    ctx.request_repaint();
                }
                Ok(frame) => {
                    match frame {
                        OutboundFrame::Stop => time_sync.mode_stopped(Instant::now()),
                        OutboundFrame::ContinueConstantCurrentDischarge(..)
                        | OutboundFrame::ContinueConstantPowerDischarge(..)
                        | OutboundFrame::ContinueConstantVoltageCharge(..) => {
                            time_sync.mode_continued(Instant::now());
                        }
                        OutboundFrame::StartConstantCurrentDischarge(..)
                        | OutboundFrame::StartConstantPowerDischarge(..)
                        | OutboundFrame::StartConstantVoltageCharge(..) => {
                            time_sync.mode_started(Instant::now());
                        }
                        _ => {}
                    }
                    if let Some(ref mut p) = port {
                        let bytes: [u8; OUTBOUND_FRAME_SIZE] = frame.into();
                        if let Err(e) = p.write_all(&bytes) {
                            log::error!("Failed to send frame: {e}");
                        }
                    }
                }
                Err(_) => break,
            }
        }

        send_time_sync_if_needed(&mut time_sync, &mut port, &event_tx, &ctx);

        // Read any available data from the device, and process it into frames.
        if let Some(ref mut p) = port {
            let mut temp_buffer = [0u8; 64];
            match p.read(&mut temp_buffer) {
                Ok(n) if n > 0 => {
                    buffer.extend_from_slice(&temp_buffer[..n]);
                    for (frame, raw) in crate::device::process_buffer(&mut buffer) {
                        event_tx.unbounded_send(DeviceEvent::Frame(frame, raw)).ok();
                        ctx.request_repaint();
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    log::error!("Serial read error: {e}");
                    port = None;
                    buffer.clear();
                    event_tx
                        .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Error(
                            "Read error: connection lost".to_owned(),
                        )))
                        .ok();
                    time_sync.mode_stopped(Instant::now());
                    ctx.request_repaint();
                }
            }
        } else {
            std::thread::sleep(SLEEP_DURATION);
        }
    }
}
