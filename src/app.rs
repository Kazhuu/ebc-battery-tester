use crate::usb;
use std::{cell::RefCell, rc::Rc};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MainApp {
    #[serde(skip)]
    usb: Rc<RefCell<usb::UsbState>>,
}

impl Default for MainApp {
    fn default() -> Self {
        Self {
            usb: Default::default(),
        }
    }
}

impl MainApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let app: Self = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        };

        usb::enumerate_devices(
            Rc::clone(&app.usb),
            cc.egui_ctx.clone(),
        );

        app
    }

    fn usb_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("USB Device");

        let device_labels: Vec<String> = self
            .usb
            .borrow()
            .available_devices
            .iter()
            .map(|d| d.to_string())
            .collect();

        let selected_text = self
            .usb
            .borrow()
            .selected_index
            .and_then(|i| device_labels.get(i))
            .map_or_else(|| "No device selected".to_owned(), Clone::clone);

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("usb_device_selector")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    if device_labels.is_empty() {
                        ui.label("No devices found");
                    }
                    for (i, label) in device_labels.iter().enumerate() {
                        ui.selectable_value(&mut self.usb.borrow_mut().selected_index, Some(i), label);
                    }
                });

            if ui.button("Refresh").clicked() {
                usb::enumerate_devices(Rc::clone(&self.usb), ui.ctx().clone());
            }
            if ui.button("Pair new device").clicked() {
                usb::request_device(Rc::clone(&self.usb), ui.ctx().clone());
            }
        });

        ui.horizontal(|ui| {
            let index = self.usb.borrow().selected_index;
            let status = self.usb.borrow().status.clone();
            match status {
                usb::ConnectionStatus::Disconnected => {
                    ui.label("Status: Disconnected");
                    if let Some(idx) = index {
                        if ui.button("Connect").clicked() {
                            usb::connect(idx, Rc::clone(&self.usb), ui.ctx().clone());
                        }
                    }
                }
                usb::ConnectionStatus::Connecting => {
                    ui.spinner();
                    ui.label("Connecting...");
                }
                usb::ConnectionStatus::Connected => {
                    ui.colored_label(egui::Color32::GREEN, "Status: Connected");
                    if ui.button("Disconnect").clicked() {
                        usb::disconnect(Rc::clone(&self.usb), ui.ctx().clone());
                    }
                }
                usb::ConnectionStatus::Error(msg) => {
                    ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
                    if let Some(idx) = index {
                        if ui.button("Retry").clicked() {
                            usb::connect(idx, Rc::clone(&self.usb), ui.ctx().clone());
                        }
                    }
                }
            }
        });
    }
}

impl eframe::App for MainApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ui.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }
                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.usb_ui(ui);

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
