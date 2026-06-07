use crate::device::{DeviceEvent, OutboundFrame};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

pub fn enumerate_devices(event_tx: UnboundedSender<DeviceEvent>) {
    todo!()
}

pub fn request_device(event_tx: UnboundedSender<DeviceEvent>) {
    todo!()
}

pub fn spawn_device_task(
    ctx: egui::Context,
    cmd_rx: UnboundedReceiver<OutboundFrame>,
    event_tx: UnboundedSender<DeviceEvent>,
) {
    todo!()
}
