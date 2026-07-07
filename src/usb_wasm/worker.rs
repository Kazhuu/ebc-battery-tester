use super::connection;
use crate::device::{ConnectionStatus, DeviceEvent, OUTBOUND_FRAME_SIZE, OutboundFrame};
use futures::FutureExt as _;
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::channel::oneshot;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

const INBOUND_BUFFER_SIZE: u32 = 64;

pub(super) async fn device_task(
    ctx: egui::Context,
    mut cmd_rx: UnboundedReceiver<OutboundFrame>,
    event_tx: UnboundedSender<DeviceEvent>,
) {
    let mut stop_reading_tx: Option<oneshot::Sender<()>> = None;
    let mut device: Option<web_sys::UsbDevice> = None;
    let mut out_endpoint_num: Option<u8> = None;
    loop {
        match cmd_rx.next().await {
            Some(OutboundFrame::Connect(idx)) => {
                event_tx
                    .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Connecting))
                    .ok();
                ctx.request_repaint();
                match connection::connect(idx).await {
                    Ok(state) => {
                        let dev = state.device;
                        out_endpoint_num = Some(state.out_endpoint_num);
                        let (stop_tx, stop_rx) = oneshot::channel();
                        stop_reading_tx = Some(stop_tx);
                        event_tx
                            .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Connected))
                            .ok();
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
                        event_tx
                            .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Error(e)))
                            .ok();
                        ctx.request_repaint();
                    }
                }
            }
            Some(OutboundFrame::Disconnect) => {
                if let (Some(device), Some(ep)) = (&device, out_endpoint_num) {
                    if let Some(stop_tx) = stop_reading_tx.take() {
                        let result = stop_tx.send(());
                        if let Err(e) = result {
                            log::error!("Failed to stop reading task: {e:?}");
                        }
                    }
                    let result = connection::disconnect(device, ep).await;
                    if let Err(e) = result {
                        log::error!("Failed to disconnect: {e:?}");
                    }
                }
                device = None;
                out_endpoint_num = None;
                event_tx
                    .unbounded_send(DeviceEvent::StatusChanged(ConnectionStatus::Disconnected))
                    .ok();
                ctx.request_repaint();
            }
            Some(OutboundFrame::Stop) => {
                if let (Some(device), Some(ep)) = (&device, out_endpoint_num) {
                    let result = connection::stop(device, ep).await;
                    if let Err(e) = result {
                        log::error!("Failed to send stop command: {e:?}");
                    }
                }
            }
            // Every other frame (discharge/charge start/adjust/continue, timer
            // sync, calibration) is a plain "encode and transfer" command with
            // no extra connection-state bookkeeping, so they all funnel through
            // send_frame.
            Some(frame) => {
                if let (Some(device), Some(ep)) = (&device, out_endpoint_num)
                    && let Err(e) = send_frame(device, ep, frame.clone()).await
                {
                    log::error!("Failed to send {frame:?}: {e:?}");
                }
            }
            None => break,
        }
    }
}

async fn send_frame(
    device: &web_sys::UsbDevice,
    out_endpoint_num: u8,
    frame: OutboundFrame,
) -> Result<(), JsValue> {
    let mut bytes: [u8; OUTBOUND_FRAME_SIZE] = frame.into();
    let promise = device
        .transfer_out_with_u8_slice(out_endpoint_num, &mut bytes)
        .map_err(|e| format!("Failed to start transfer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Frame send failed: {e:?}"))?;
    Ok(())
}

async fn reading_task(
    device: web_sys::UsbDevice,
    in_endpoint: u8,
    event_tx: UnboundedSender<DeviceEvent>,
    mut stop_reading_rx: oneshot::Receiver<()>,
    ctx: egui::Context,
) {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let transfer = JsFuture::from(device.transfer_in(in_endpoint, INBOUND_BUFFER_SIZE)).fuse();
        futures::pin_mut!(transfer);
        futures::select! {
            result = transfer => match result {
                Ok(value) => {
                    let result: web_sys::UsbInTransferResult = value.unchecked_into();
                    if let Some(data) = result.data() {
                        buf.extend_from_slice(&js_sys::Uint8Array::new(&data.buffer()).to_vec());
                        for (frame, raw) in crate::device::process_buffer(&mut buf) {
                            event_tx.unbounded_send(DeviceEvent::Frame(frame, raw)).ok();
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
