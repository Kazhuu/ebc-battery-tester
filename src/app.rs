use crate::usb;
use std::{cell::RefCell, rc::Rc};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MainApp {
    #[serde(skip)]
    usb_devices: Rc<RefCell<Vec<usb::UsbDeviceInfo>>>,

    #[serde(skip)]
    selected_device_idx: Option<usize>,
}

impl Default for MainApp {
    fn default() -> Self {
        Self {
            usb_devices: Default::default(),
            selected_device_idx: None,
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
            Rc::clone(&app.usb_devices),
            cc.egui_ctx.clone(),
        );

        app
    }

    fn usb_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("USB Device");

        let device_labels: Vec<String> = self
            .usb_devices
            .borrow()
            .iter()
            .map(|d| d.to_string())
            .collect();

        let selected_text = self
            .selected_device_idx
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
                        ui.selectable_value(&mut self.selected_device_idx, Some(i), label);
                    }
                });

            if ui.button("Refresh").clicked() {
                usb::enumerate_devices(
                    Rc::clone(&self.usb_devices),
                    ui.ctx().clone(),
                );
            }
            if ui.button("Pair new device").clicked() {
                usb::request_device(
                    Rc::clone(&self.usb_devices),
                    ui.ctx().clone(),
                );
            }
            if ui.button("Connect").clicked() {
                if let Some(index) = self.selected_device_idx {
                    usb::connect_and_write(index, ui.ctx().clone());
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
