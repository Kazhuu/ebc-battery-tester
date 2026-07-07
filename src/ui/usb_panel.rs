use crate::device::{ConnectionStatus, OutboundFrame};
use crate::session::DeviceSession;
use crate::usb;

pub(crate) fn ui(session: &mut DeviceSession, ui: &mut egui::Ui) {
    ui.heading("USB Device");

    let device_labels: Vec<String> = session
        .available_devices
        .iter()
        .map(|d| d.to_string())
        .collect();

    let selected_text = session
        .selected_device_index
        .and_then(|i| device_labels.get(i))
        .map_or_else(|| "No device selected".to_owned(), Clone::clone);

    ui.horizontal(|ui| {
        #[cfg(target_arch = "wasm32")]
        if ui.button("+").clicked() {
            usb::request_device(session.event_tx.clone());
        }
        if ui.button("⟳").clicked() {
            usb::enumerate_devices(session.event_tx.clone());
        }
        egui::ComboBox::from_id_salt("usb_device_selector")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                if device_labels.is_empty() {
                    ui.label("No devices found");
                }
                for (i, label) in device_labels.iter().enumerate() {
                    ui.selectable_value(&mut session.selected_device_index, Some(i), label);
                }
            });
        match &session.status {
            ConnectionStatus::Disconnected => {
                if let Some(idx) = session.selected_device_index
                    && ui.button("Connect").clicked()
                {
                    session.send_cmd(OutboundFrame::Connect(idx), ui.ctx());
                }
            }
            ConnectionStatus::Connecting => {
                ui.spinner();
                ui.label("Connecting...");
            }
            ConnectionStatus::Connected => {
                if ui.button("Disconnect").clicked() {
                    session.send_cmd(OutboundFrame::Stop, ui.ctx());
                    session.send_cmd(OutboundFrame::Disconnect, ui.ctx());
                }
            }
            ConnectionStatus::Error(_) => {
                if let Some(idx) = session.selected_device_index
                    && ui.button("Retry").clicked()
                {
                    session.send_cmd(OutboundFrame::Connect(idx), ui.ctx());
                }
            }
        }
    });
    if let ConnectionStatus::Error(msg) = &session.status {
        ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
    }
}
