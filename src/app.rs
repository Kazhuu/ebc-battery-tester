use crate::device::ConnectionStatus;
use crate::session::DeviceSession;
use crate::ui;

#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct MainApp {
    control_panel: ui::control_panel::ControlPanel,
    #[serde(skip)]
    session: DeviceSession,
    #[serde(skip)]
    calibrate_window: ui::calibrate_window::CalibrateWindow,
    #[serde(skip)]
    log_window: ui::log_window::LogWindow,
    #[serde(skip)]
    about_window: ui::about_window::AboutWindow,
}

impl MainApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app: Self = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        };
        app.session = DeviceSession::new(&cc.egui_ctx);
        app.about_window = ui::about_window::AboutWindow::new(&cc.egui_ctx);
        app
    }
}

impl eframe::App for MainApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    // On application exit, send a Stop and Disconnect command to the device.
    // Sending stop will stop the current mode if it is running and sending
    // disconnect will disconnect the device. This is important to do so that
    // the device is not left in a running state when the application exits.
    fn on_exit(&mut self) {
        self.session.shutdown();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.about_window.poll();
        self.session.consume_events(ui.ctx());
        self.session.send_timer_sync_if_needed(ui.ctx());
        // Request a repaint every second to update the timer. This is needed so
        // that the clock is updated every second. Not when something happens.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                egui::widgets::global_theme_preference_buttons(ui);
                ui.separator();
                if ui
                    .add_enabled(
                        self.session.status == ConnectionStatus::Connected
                            && self.session.has_live_voltage(),
                        egui::Button::new("Calibrate"),
                    )
                    .on_disabled_hover_text("Connect the device to a battery first")
                    .clicked()
                {
                    self.calibrate_window.open = true;
                }
                ui.separator();
                if ui.button("Log").clicked() {
                    self.log_window.open = !self.log_window.open;
                }
                ui.separator();
                if ui.button("About").clicked() {
                    self.about_window.open = true;
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    match &self.about_window.update_check_state {
                        crate::update_check::UpdateCheckState::Checking => {
                            ui.separator();
                            ui.spinner();
                            ui.weak("Checking for updates...");
                        }
                        crate::update_check::UpdateCheckState::UpToDate => {
                            ui.separator();
                            ui.label(format!("v{} (up to date)", env!("CARGO_PKG_VERSION")));
                        }
                        crate::update_check::UpdateCheckState::UpdateAvailable(tag) => {
                            ui.separator();
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                format!("Update available: {tag}"),
                            );
                            ui.hyperlink_to("Download", crate::update_check::RELEASES_PAGE_URL);
                        }
                        crate::update_check::UpdateCheckState::Failed => {}
                    }
                }
            });
        });

        egui::Panel::left("left_panel").show_inside(ui, |ui| {
            self.about_window.ui(ui);
            self.calibrate_window.ui(&mut self.session, ui);
            self.log_window.ui(&mut self.session, ui);
            ui::usb_panel::ui(&mut self.session, ui);
            ui.push_id("control_section", |ui| {
                if self.session.status == ConnectionStatus::Connected {
                    ui::live_data::ui(&self.session, ui);
                    self.control_panel.ui(&mut self.session, ui);
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui::plot::ui(&self.session, ui);
        });
    }
}
