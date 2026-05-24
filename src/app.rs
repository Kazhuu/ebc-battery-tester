use crate::device;
use crate::usb;
use device::{ConnectionStatus, DeviceCommand, DeviceEvent};
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use std::collections::VecDeque;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MainApp {
    #[serde(skip)]
    available_devices: Vec<device::UsbDeviceInfo>,
    #[serde(skip)]
    selected_device_index: Option<usize>,
    #[serde(skip)]
    cmd_tx: UnboundedSender<DeviceCommand>,
    #[serde(skip)]
    event_rx: UnboundedReceiver<DeviceEvent>,
    #[serde(skip)]
    event_tx: UnboundedSender<DeviceEvent>,
    #[serde(skip)]
    status: ConnectionStatus,
    #[serde(skip)]
    current_device_mode: device::DeviceMode,
    #[serde(skip)]
    mode_on: bool,
    current: f32,
    voltage: f32,
    watts: u16,
    time: u16,
    #[serde(skip)]
    frames: Vec<VecDeque<u8>>,
}

impl Default for MainApp {
    fn default() -> Self {
        Self {
            available_devices: Default::default(),
            selected_device_index: None,
            cmd_tx: mpsc::unbounded::<DeviceCommand>().0,
            event_rx: mpsc::unbounded::<DeviceEvent>().1,
            event_tx: mpsc::unbounded::<DeviceEvent>().0,
            status: ConnectionStatus::Disconnected,
            current_device_mode: device::DeviceMode::DischargeConstantCurrent,
            mode_on: false,
            current: 0.0,
            voltage: 0.0,
            watts: 0,
            time: 0,
            frames: Vec::new(),
        }
    }
}

impl MainApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app: Self = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        };
        let (cmd_tx, cmd_rx) = mpsc::unbounded::<DeviceCommand>();
        let (event_tx, event_rx) = mpsc::unbounded::<DeviceEvent>();
        usb::spawn_device_task(cc.egui_ctx.clone(), cmd_rx, event_tx.clone());
        usb::enumerate_devices(event_tx.clone());
        app.cmd_tx = cmd_tx;
        app.event_rx = event_rx;
        app.event_tx = event_tx;
        app
    }

    fn usb_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("USB Device");

        let device_labels: Vec<String> = self
            .available_devices
            .iter()
            .map(|d| d.to_string())
            .collect();

        let selected_text = self
            .selected_device_index
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
                        ui.selectable_value(&mut self.selected_device_index, Some(i), label);
                    }
                });

            if ui.button("Refresh").clicked() {
                usb::enumerate_devices(self.event_tx.clone());
            }
            if ui.button("Pair new device").clicked() {
                usb::request_device(self.event_tx.clone());
            }
        });

        ui.horizontal(|ui: &mut egui::Ui| match &self.status {
            ConnectionStatus::Disconnected => {
                ui.label("Status: Disconnected");
                if let Some(idx) = self.selected_device_index {
                    if ui.button("Connect").clicked() {
                        self.cmd_tx.unbounded_send(DeviceCommand::Connect(idx)).ok();
                    }
                }
            }
            ConnectionStatus::Connecting => {
                ui.spinner();
                ui.label("Connecting...");
            }
            ConnectionStatus::Connected => {
                ui.colored_label(egui::Color32::GREEN, "Status: Connected");
                if ui.button("Disconnect").clicked() {
                    self.cmd_tx.unbounded_send(DeviceCommand::Disconnect).ok();
                }
            }
            ConnectionStatus::Error(msg) => {
                ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
                if let Some(idx) = self.selected_device_index {
                    if ui.button("Retry").clicked() {
                        self.cmd_tx.unbounded_send(DeviceCommand::Connect(idx)).ok();
                    }
                }
            }
        });
    }

    fn control_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Control");
        ui.label("Device Mode:");
        egui::ComboBox::from_id_salt("device_mode_selector")
            .selected_text(self.current_device_mode.to_string())
            .show_ui(ui, |ui| {
                for mode in [
                    device::DeviceMode::DischargeConstantCurrent,
                    device::DeviceMode::DischargeConstantPower,
                    device::DeviceMode::ChargeConstantVoltage,
                ] {
                    ui.selectable_value(&mut self.current_device_mode, mode, mode.to_string());
                }
            });
        match self.current_device_mode {
            device::DeviceMode::DischargeConstantCurrent => {
                ui.label("Discharge Current:");
                ui.add(
                    egui::Slider::new(
                        &mut self.current,
                        device::MIN_CURRENT_MA as f32 / 1000.0
                            ..=device::MAX_CURRENT_MA as f32 / 1000.0,
                    )
                    .suffix(" A"),
                );
                ui.label("Cutoff Voltage:");
                ui.add(
                    egui::Slider::new(
                        &mut self.voltage,
                        device::MIN_VOLTAGE_MV as f32 / 1000.0
                            ..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                    )
                    .suffix(" V"),
                );
                ui.label("Cutoff Time:");
                ui.add(
                    egui::Slider::new(
                        &mut self.time,
                        device::MIN_CUTOFF_TIME_MIN..=device::MAX_CUTOFF_TIME_MIN,
                    )
                    .suffix(" min")
                    .text("Indefinite if 0"),
                );
                if self.mode_on {
                    ui.colored_label(egui::Color32::GREEN, "ON");
                    if ui.button("Stop").clicked() {
                        self.cmd_tx.unbounded_send(DeviceCommand::Stop).ok();
                        self.mode_on = false;
                    }
                } else {
                    ui.colored_label(egui::Color32::RED, "OFF");
                    if ui.button("Start").clicked() {
                        self.cmd_tx
                            .unbounded_send(DeviceCommand::StartConstantCurrentDischarge(
                                (self.current * 1000.0) as u16,
                                (self.voltage * 1000.0) as u16,
                                self.time,
                            ))
                            .ok();
                        self.mode_on = true;
                    }
                }
                if ui.button("Continue").clicked() {
                    self.cmd_tx.unbounded_send(DeviceCommand::Continue).ok();
                }
                if ui.button("Stop Discharge").clicked() {
                    self.cmd_tx
                        .unbounded_send(DeviceCommand::StopConstantCurrentDischarge)
                        .ok();
                }
            }
            device::DeviceMode::DischargeConstantPower => {
                ui.label("Discharge Power:");
                ui.add(
                    egui::Slider::new(
                        &mut self.watts,
                        device::MIN_POWER_W..=device::MAX_POWER_W,
                    )
                    .suffix(" W"),
                );
                ui.label("Cutoff Voltage:");
                ui.add(
                    egui::Slider::new(
                        &mut self.voltage,
                        device::MIN_VOLTAGE_MV as f32 / 1000.0
                            ..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                    )
                    .suffix(" V"),
                );
                ui.label("Cutoff Time:");
                ui.add(
                    egui::Slider::new(
                        &mut self.time,
                        device::MIN_CUTOFF_TIME_MIN..=device::MAX_CUTOFF_TIME_MIN,
                    )
                    .suffix(" min")
                    .text("Indefinite if 0"),
                );
                if self.mode_on {
                    ui.colored_label(egui::Color32::GREEN, "ON");
                    if ui.button("Stop").clicked() {
                        self.cmd_tx.unbounded_send(DeviceCommand::Stop).ok();
                        self.mode_on = false;
                    }
                } else {
                    ui.colored_label(egui::Color32::RED, "OFF");
                    if ui.button("Start").clicked() {
                        self.cmd_tx
                            .unbounded_send(DeviceCommand::StartConstantPowerDischarge(
                                self.watts,
                                (self.voltage * 1000.0) as u16,
                                self.time,
                            ))
                            .ok();
                        self.mode_on = true;
                    }
                }
                if ui.button("Continue").clicked() {
                    self.cmd_tx.unbounded_send(DeviceCommand::Continue).ok();
                }
            }
            device::DeviceMode::ChargeConstantVoltage => {
                ui.label("TODO");
            }
        }
    }
}

impl eframe::App for MainApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Drain all events from the device task and update the app state accordingly.
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                DeviceEvent::StatusChanged(status) => {
                    log::info!("Device status changed: {:?}", status);
                    self.status = status;
                }
                DeviceEvent::DevicesUpdated(devices) => {
                    log::info!("Available devices updated: {:?}", devices);
                    self.available_devices = devices;
                }
                DeviceEvent::Frame(frame) => {
                    log::info!("Received frame: {:?}", frame);
                    self.frames.push(VecDeque::from(frame));
                }
            }
        }
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
            if self.status == ConnectionStatus::Connected {
                //self.control_ui(ui);
            }
            self.control_ui(ui);
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
