use crate::device;
use crate::usb;
use device::{ConnectionStatus, DeviceEvent, OutboundFrame};
use egui_plot::Legend;
use egui_plot::Line;
use egui_plot::Plot;
use egui_plot::PlotPoint;
use egui_plot::PlotPoints;
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MainApp {
    #[serde(skip)]
    available_devices: Vec<device::UsbDeviceInfo>,
    #[serde(skip)]
    selected_device_index: Option<usize>,
    #[serde(skip)]
    cmd_tx: UnboundedSender<OutboundFrame>,
    #[serde(skip)]
    event_rx: UnboundedReceiver<DeviceEvent>,
    #[serde(skip)]
    event_tx: UnboundedSender<DeviceEvent>,
    #[serde(skip)]
    status: ConnectionStatus,
    current_device_mode: device::DeviceMode,
    #[serde(skip)]
    firmware_version: Option<String>,
    #[serde(skip)]
    model_name: Option<String>,
    #[serde(skip)]
    live_voltage_mv: u16,
    #[serde(skip)]
    live_current_ma: u16,
    #[serde(skip)]
    live_milli_ampere_hours: u16,
    #[serde(skip)]
    voltage_points: Vec<PlotPoint>,
    #[serde(skip)]
    amperes_points: Vec<PlotPoint>,
    #[serde(skip)]
    mode_start_time: Option<f64>,
    mode_on: bool,
    discharge_current: f32,
    discharge_cutoff_voltage: f32,
    discharge_watts: u16,
    discharge_time: u16,
    charge_current: f32,
    charge_voltage: f32,
    charge_cutoff_current: f32,
}

impl Default for MainApp {
    fn default() -> Self {
        Self {
            available_devices: Default::default(),
            selected_device_index: None,
            cmd_tx: mpsc::unbounded::<OutboundFrame>().0,
            event_rx: mpsc::unbounded::<DeviceEvent>().1,
            event_tx: mpsc::unbounded::<DeviceEvent>().0,
            status: ConnectionStatus::Disconnected,
            current_device_mode: device::DeviceMode::DischargeConstantCurrent,
            mode_on: false,
            firmware_version: None,
            model_name: None,
            charge_current: 0.0,
            charge_cutoff_current: 0.0,
            charge_voltage: 0.0,
            discharge_current: 0.0,
            discharge_cutoff_voltage: 0.0,
            discharge_watts: 0,
            discharge_time: 0,
            live_voltage_mv: 0,
            live_current_ma: 0,
            live_milli_ampere_hours: 0,
            voltage_points: Vec::new(),
            amperes_points: Vec::new(),
            mode_start_time: None,
        }
    }
}

fn has_live_voltage(state: &MainApp) -> bool {
    state.live_voltage_mv > 0
}

impl MainApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app: Self = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        };
        let (cmd_tx, cmd_rx) = mpsc::unbounded::<OutboundFrame>();
        let (event_tx, event_rx) = mpsc::unbounded::<DeviceEvent>();
        usb::spawn_device_task(cc.egui_ctx.clone(), cmd_rx, event_tx.clone());
        usb::enumerate_devices(event_tx.clone());
        app.cmd_tx = cmd_tx;
        app.event_rx = event_rx;
        app.event_tx = event_tx;
        app
    }

    fn consume_events(&mut self, ctx: &egui::Context) {
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
                    log::info!("{:?}", DeviceEvent::Frame(frame.clone()));
                    match frame {
                        device::InboundFrame::FirmwareReport(firmware_report_struct) => {
                            self.live_voltage_mv = firmware_report_struct.voltage_mv;
                            self.live_current_ma = firmware_report_struct.current_ma;
                            self.live_milli_ampere_hours =
                                firmware_report_struct.milli_ampere_hours;
                            self.firmware_version = Some(firmware_report_struct.firmware_version);
                            self.model_name = Some(firmware_report_struct.device_type);
                        }
                        device::InboundFrame::ChargeReport(cccv_report_struct) => {
                            self.mode_on = cccv_report_struct.in_progress;
                            self.live_voltage_mv = cccv_report_struct.voltage_mv;
                            self.live_current_ma = cccv_report_struct.current_ma;
                            self.live_milli_ampere_hours = cccv_report_struct.milli_ampere_hours;
                            self.voltage_points.push(PlotPoint::new(
                                self.mode_start_time
                                    .map(|start| ctx.input(|ui| ui.time) - start)
                                    .unwrap_or(0.0),
                                self.live_voltage_mv as f64 / 1000.0,
                            ));
                            self.amperes_points.push(PlotPoint::new(
                                self.mode_start_time
                                    .map(|start| ctx.input(|ui| ui.time) - start)
                                    .unwrap_or(0.0),
                                self.live_current_ma as f64 / 1000.0,
                            ));
                        }
                        device::InboundFrame::DischargeConstantCurrentReport(
                            discharge_report_struct,
                        ) => {
                            self.mode_on = discharge_report_struct.in_progress;
                            self.live_voltage_mv = discharge_report_struct.voltage_mv;
                            self.live_current_ma = discharge_report_struct.current_ma;
                            self.live_milli_ampere_hours =
                                discharge_report_struct.milli_ampere_hours;
                        }
                        device::InboundFrame::DischargeConstantPowerReport(
                            discharge_report_struct,
                        ) => {
                            self.mode_on = discharge_report_struct.in_progress;
                            self.live_voltage_mv = discharge_report_struct.voltage_mv;
                            self.live_current_ma = discharge_report_struct.current_ma;
                            self.live_milli_ampere_hours =
                                discharge_report_struct.milli_ampere_hours;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn live_data_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            if let Some(model_name) = &self.model_name {
                ui.label(format!("Model: {model_name}"));
            } else {
                ui.label("Model: --");
            }
            if let Some(firmware_version) = &self.firmware_version {
                ui.label(format!("Firmware: v{firmware_version}"));
            } else {
                ui.label("Firmware: --");
            }
        });
        ui.horizontal(|ui| {
            ui.label("Status: ");
            if self.mode_on {
                ui.colored_label(ui.visuals().warn_fg_color, "Running");
            } else {
                ui.label("Idle");
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!("{:.3} V", self.live_voltage_mv as f32 / 1000.0));
            ui.label(format!("{:.2} A", self.live_current_ma as f32 / 1000.0));
            ui.label(format!("{:.2} mAh", self.live_milli_ampere_hours));
            if !has_live_voltage(self) {
                ui.colored_label(ui.visuals().error_fg_color, "Connect the device to a battery");
            }
        });
    }

    fn usb_ui(&mut self, ui: &mut egui::Ui) {
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
                        self.cmd_tx.unbounded_send(OutboundFrame::Connect(idx)).ok();
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
                    self.cmd_tx.unbounded_send(OutboundFrame::Disconnect).ok();
                }
            }
            ConnectionStatus::Error(msg) => {
                ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
                if let Some(idx) = self.selected_device_index {
                    if ui.button("Retry").clicked() {
                        self.cmd_tx.unbounded_send(OutboundFrame::Connect(idx)).ok();
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
                        &mut self.discharge_current,
                        device::MIN_DISCHARGE_CURRENT_MA as f32 / 1000.0
                            ..=device::MAX_DISCHARGE_CURRENT_MA as f32 / 1000.0,
                    )
                    .suffix(" A"),
                );
                ui.label("Cutoff Voltage:");
                ui.add(
                    egui::Slider::new(
                        &mut self.discharge_cutoff_voltage,
                        device::MIN_VOLTAGE_MV as f32 / 1000.0
                            ..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                    )
                    .suffix(" V"),
                );
                ui.label("Cutoff Time:");
                ui.add(
                    egui::Slider::new(
                        &mut self.discharge_time,
                        device::MIN_CUTOFF_TIME_MIN..=device::MAX_CUTOFF_TIME_MIN,
                    )
                    .suffix(" min")
                    .text("Indefinite if 0"),
                );
                ui.horizontal(|ui| {
                    if self.mode_on {
                        if ui.button("Stop").clicked() {
                            self.cmd_tx.unbounded_send(OutboundFrame::Stop).ok();
                            self.mode_on = false;
                        }
                    } else {
                        if ui
                            .add_enabled(has_live_voltage(self), egui::Button::new("Start"))
                            .on_disabled_hover_text("Connect device to battery first")
                            .clicked()
                        {
                            self.cmd_tx
                                .unbounded_send(OutboundFrame::StartConstantCurrentDischarge(
                                    (self.discharge_current * 1000.0) as u16,
                                    (self.discharge_cutoff_voltage * 1000.0) as u16,
                                    self.discharge_time,
                                ))
                                .ok();
                            self.mode_on = true;
                        }
                    }
                });
            }
            device::DeviceMode::DischargeConstantPower => {
                ui.label("Discharge Power:");
                ui.add(
                    egui::Slider::new(
                        &mut self.discharge_watts,
                        device::MIN_POWER_W..=device::MAX_POWER_W,
                    )
                    .suffix(" W"),
                );
                ui.label("Cutoff Voltage:");
                ui.add(
                    egui::Slider::new(
                        &mut self.discharge_cutoff_voltage,
                        device::MIN_VOLTAGE_MV as f32 / 1000.0
                            ..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                    )
                    .suffix(" V"),
                );
                ui.label("Cutoff Time:");
                ui.add(
                    egui::Slider::new(
                        &mut self.discharge_time,
                        device::MIN_CUTOFF_TIME_MIN..=device::MAX_CUTOFF_TIME_MIN,
                    )
                    .suffix(" min")
                    .text("Indefinite if 0"),
                );
                if self.mode_on {
                    if ui.button("Stop").clicked() {
                        self.cmd_tx.unbounded_send(OutboundFrame::Stop).ok();
                        self.mode_on = false;
                    }
                } else {
                    if ui
                        .add_enabled(has_live_voltage(self), egui::Button::new("Start"))
                        .on_disabled_hover_text("Connect device to battery first")
                        .clicked()
                    {
                        self.cmd_tx
                            .unbounded_send(OutboundFrame::StartConstantPowerDischarge(
                                self.discharge_watts,
                                (self.discharge_cutoff_voltage * 1000.0) as u16,
                                self.discharge_time,
                            ))
                            .ok();
                        self.mode_on = true;
                    }
                }
            }
            device::DeviceMode::ChargeConstantVoltage => {
                ui.label("Charge Current:");
                ui.add(
                    egui::Slider::new(
                        &mut self.charge_current,
                        device::MIN_CHARGE_CURRENT_MA as f32 / 1000.0
                            ..=device::MAX_CHARGE_CURRENT_MA as f32 / 1000.0,
                    )
                    .suffix(" A"),
                );
                ui.label("Charge Voltage:");
                ui.add(
                    egui::Slider::new(
                        &mut self.charge_voltage,
                        device::MIN_VOLTAGE_MV as f32 / 1000.0
                            ..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                    )
                    .suffix(" V"),
                );
                ui.label("Cutoff Current:");
                ui.add(
                    egui::Slider::new(
                        &mut self.charge_cutoff_current,
                        device::MIN_CHARGE_CUTOFF_CURRENT_MA as f32 / 1000.0
                            ..=device::MAX_CHARGE_CUTOFF_CURRENT_MA as f32 / 1000.0,
                    )
                    .suffix(" A"),
                );
                if self.mode_on {
                    if ui.button("Stop").clicked() {
                        self.cmd_tx.unbounded_send(OutboundFrame::Stop).ok();
                        self.mode_on = false;
                    }
                } else {
                    if ui
                        .add_enabled(has_live_voltage(self), egui::Button::new("Start"))
                        .on_disabled_hover_text("Connect device to battery first")
                        .clicked()
                    {
                        self.cmd_tx
                            .unbounded_send(OutboundFrame::StartConstantVoltageCharge(
                                (self.charge_current * 1000.0) as u16,
                                (self.charge_voltage * 1000.0) as u16,
                                (self.charge_cutoff_current * 1000.0) as u16,
                            ))
                            .ok();
                        self.mode_start_time = Some(ui.ctx().input(|ui| ui.time));
                        self.mode_on = true;
                        self.voltage_points.clear();
                        self.amperes_points.clear();
                    }
                }
            }
        }
    }

    fn plot_ui(&mut self, ui: &mut egui::Ui) {
        let label_formatter = |_s: &str, val: &PlotPoint| {
            format!(
                "{:.2} s: {:.3} V, {:.2} A",
                val.x,
                val.y,
                self.live_current_ma as f64 / 1000.0
            )
        };

        Plot::new("live_data_plot")
            .legend(Legend::default())
            .label_formatter(label_formatter)
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new("Voltage", PlotPoints::Borrowed(&self.voltage_points))
                        .name("Voltage"),
                );
                plot_ui.line(
                    Line::new("Current", PlotPoints::Borrowed(&self.amperes_points))
                        .name("Current"),
                );
            });
    }
}

impl eframe::App for MainApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.consume_events(ui.ctx());
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
            self.live_data_ui(ui);
            if self.status == ConnectionStatus::Connected {
                self.control_ui(ui);
            }
            self.plot_ui(ui);
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
