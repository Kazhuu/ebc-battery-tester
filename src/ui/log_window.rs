use crate::export::{LogDirection, save_log_to_file};
use crate::session::DeviceSession;

#[derive(Default)]
pub(crate) struct LogWindow {
    pub(crate) open: bool,
}

impl LogWindow {
    pub(crate) fn ui(&mut self, session: &mut DeviceSession, ui: &egui::Ui) {
        if !self.open {
            return;
        }
        egui::Window::new("Frame Log")
            .open(&mut self.open)
            .resizable(true)
            .default_size([500.0, 600.0])
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} entries", session.log_entries.len()));
                    if ui.button("Clear").clicked() {
                        session.log_entries.clear();
                    }
                    if ui.button("Save").clicked() {
                        save_log_to_file(&session.log_entries);
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for entry in &session.log_entries {
                            let (prefix, color) = match entry.direction {
                                LogDirection::In => ("IN ", egui::Color32::from_rgb(100, 200, 100)),
                                LogDirection::Out => {
                                    ("OUT", egui::Color32::from_rgb(100, 150, 255))
                                }
                            };
                            let hex: String = entry
                                .raw_bytes
                                .iter()
                                .map(|b| format!("{b:02x}"))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let t = entry.timestamp as u64;
                            let hours = t / 3600;
                            let mins = (t % 3600) / 60;
                            let secs = t % 60;
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{hours:02}:{mins:02}:{secs:02}"))
                                        .monospace()
                                        .weak(),
                                );
                                ui.colored_label(color, egui::RichText::new(prefix).monospace());
                                ui.label(&entry.label);
                            });
                            ui.label(
                                egui::RichText::new(format!("             {hex}"))
                                    .monospace()
                                    .weak(),
                            );
                            ui.add_space(2.0);
                        }
                    });
            });
    }
}
