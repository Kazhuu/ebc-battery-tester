use crate::device;
use crate::usb;
use device::{ConnectionStatus, DeviceEvent, OutboundFrame};
use egui_plot::AxisHints;
use egui_plot::HPlacement;
use egui_plot::Legend;
use egui_plot::Line;
use egui_plot::Plot;
use egui_plot::PlotPoint;
use egui_plot::PlotPoints;
use egui_plot::VPlacement;
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
    #[serde(skip)]
    current_device_mode: Option<device::DeviceMode>,
    #[serde(skip)]
    mode_on: bool,
    selected_device_mode: device::DeviceMode,
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
            selected_device_mode: device::DeviceMode::DischargeConstantCurrent,
            current_device_mode: None,
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
        usb::spawn_device_worker(cc.egui_ctx.clone(), cmd_rx, event_tx.clone());
        usb::enumerate_devices(event_tx.clone());
        app.cmd_tx = cmd_tx;
        app.event_rx = event_rx;
        app.event_tx = event_tx;
        app
    }

    fn handle_firmware_report(&mut self, firmware_report_struct: device::FirmwareReport) {
        self.current_device_mode = Some(firmware_report_struct.device_mode);
        self.mode_on = firmware_report_struct.in_progress;
        self.live_voltage_mv = firmware_report_struct.voltage_mv;
        self.live_current_ma = firmware_report_struct.current_ma;
        self.live_milli_ampere_hours = firmware_report_struct.milli_ampere_hours;
        self.firmware_version = Some(firmware_report_struct.firmware_version);
        self.model_name = Some(firmware_report_struct.device_type);
    }

    fn handle_charge_report(&mut self, charge_report: device::ChargeReport, ctx: &egui::Context) {
        self.current_device_mode = Some(device::DeviceMode::ChargeConstantVoltage);
        self.mode_on = charge_report.in_progress;
        self.live_voltage_mv = charge_report.voltage_mv;
        self.live_current_ma = charge_report.current_ma;
        self.live_milli_ampere_hours = charge_report.milli_ampere_hours;
        self.model_name = Some(charge_report.device_type);
        if self.mode_on {
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
    }

    fn handle_discharge_constant_current_report(
        &mut self,
        discharge_report_struct: device::DischargeConstantCurrentReport,
        ctx: &egui::Context,
    ) {
        self.current_device_mode = Some(device::DeviceMode::DischargeConstantCurrent);
        self.mode_on = discharge_report_struct.in_progress;
        self.live_voltage_mv = discharge_report_struct.voltage_mv;
        self.live_current_ma = discharge_report_struct.current_ma;
        self.live_milli_ampere_hours = discharge_report_struct.milli_ampere_hours;
        self.model_name = Some(discharge_report_struct.device_type);
        if self.mode_on {
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
    }

    fn handle_discharge_constant_power_report(
        &mut self,
        discharge_report_struct: device::DischargeConstantPowerReport,
        ctx: &egui::Context,
    ) {
        self.current_device_mode = Some(device::DeviceMode::DischargeConstantPower);
        self.mode_on = discharge_report_struct.in_progress;
        self.live_voltage_mv = discharge_report_struct.voltage_mv;
        self.live_current_ma = discharge_report_struct.current_ma;
        self.live_milli_ampere_hours = discharge_report_struct.milli_ampere_hours;
        self.model_name = Some(discharge_report_struct.device_type);
        if self.mode_on {
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
    }

    fn consume_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                DeviceEvent::StatusChanged(status) => {
                    log::info!("Device status changed: {status:?}");
                    self.status = status;
                }
                DeviceEvent::DevicesUpdated(devices) => {
                    log::info!("Available devices updated: {devices:?}");
                    self.available_devices = devices;
                    if self.available_devices.len() == 1 {
                        self.selected_device_index = Some(0);
                    } else if let Some(selected_index) = self.selected_device_index
                        && selected_index >= self.available_devices.len()
                    {
                        self.selected_device_index = None;
                    }
                }
                DeviceEvent::Frame(frame) => {
                    log::info!("Received frame: {frame:?}");
                    match frame {
                        device::InboundFrame::Firmware(firmware_report) => {
                            self.handle_firmware_report(firmware_report);
                        }
                        device::InboundFrame::Charge(charge_report) => {
                            self.handle_charge_report(charge_report, ctx);
                        }
                        device::InboundFrame::DischargeConstantCurrent(discharge_report_struct) => {
                            self.handle_discharge_constant_current_report(
                                discharge_report_struct,
                                ctx,
                            );
                        }
                        device::InboundFrame::DischargeConstantPower(discharge_report_struct) => {
                            self.handle_discharge_constant_power_report(
                                discharge_report_struct,
                                ctx,
                            );
                        }
                    }
                }
            }
        }
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

        ui.label("Select device:");
        ui.horizontal(|ui| {
            #[cfg(target_arch = "wasm32")]
            if ui.button("+").clicked() {
                usb::request_device(self.event_tx.clone());
            }
            if ui.button("⟳").clicked() {
                usb::enumerate_devices(self.event_tx.clone());
            }
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
            match &self.status {
                ConnectionStatus::Disconnected => {
                    if let Some(idx) = self.selected_device_index
                        && ui.button("Connect").clicked()
                    {
                        self.cmd_tx.unbounded_send(OutboundFrame::Connect(idx)).ok();
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.spinner();
                    ui.label("Connecting...");
                }
                ConnectionStatus::Connected => {
                    if ui.button("Disconnect").clicked() {
                        self.cmd_tx.unbounded_send(OutboundFrame::Disconnect).ok();
                    }
                }
                ConnectionStatus::Error(msg) => {
                    // TODO: Show error somewhere else.
                    ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
                    if let Some(idx) = self.selected_device_index
                        && ui.button("Retry").clicked()
                    {
                        self.cmd_tx.unbounded_send(OutboundFrame::Connect(idx)).ok();
                    }
                }
            }
        });
    }

    fn live_data_ui(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Live Data");
        ui.horizontal(|ui| {
            ui.label("Measurements:");
            if !has_live_voltage(self) {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "Connect the device to a battery",
                );
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!("{:.3} V", self.live_voltage_mv as f32 / 1000.0));
            ui.label(format!("{:.2} A", self.live_current_ma as f32 / 1000.0));
            ui.label(format!("{:.2} mAh", self.live_milli_ampere_hours));
        });
        ui.horizontal(|ui| {
            ui.label("Mode:");
            if let Some(current_device_mode) = self.current_device_mode {
                ui.colored_label(
                    if self.mode_on {
                        ui.visuals().warn_fg_color
                    } else {
                        ui.visuals().text_color()
                    },
                    format!(
                        "{current_device_mode}{}",
                        if self.mode_on { " (On)" } else { " (Off)" }
                    ),
                );
            } else {
                ui.label("--");
            }
        });
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
    }

    fn discharge_constant_current_ui(&mut self, ui: &mut egui::Ui) {
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
                device::MIN_VOLTAGE_MV as f32 / 1000.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
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
            } else if ui
                .add_enabled(has_live_voltage(self), egui::Button::new("Start"))
                .on_disabled_hover_text("Connect the device to a battery first")
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
                self.mode_start_time = Some(ui.ctx().input(|ui| ui.time));
                self.voltage_points.clear();
                self.amperes_points.clear();
            }

            if ui
                .add_enabled(
                    has_live_voltage(self) && !self.mode_on,
                    egui::Button::new("Continue"),
                )
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
                if self.mode_start_time.is_none() {
                    self.mode_start_time = Some(ui.ctx().input(|ui| ui.time));
                }
            }
        });
    }

    fn discharge_constant_power_ui(&mut self, ui: &mut egui::Ui) {
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
                device::MIN_VOLTAGE_MV as f32 / 1000.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
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
            } else if ui
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
                self.mode_start_time = Some(ui.ctx().input(|ui| ui.time));
                self.voltage_points.clear();
                self.amperes_points.clear();
            }
            if ui
                .add_enabled(
                    has_live_voltage(self) && !self.mode_on,
                    egui::Button::new("Continue"),
                )
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
                if self.mode_start_time.is_none() {
                    self.mode_start_time = Some(ui.ctx().input(|ui| ui.time));
                }
            }
        });
    }

    fn charge_constant_voltage_ui(&mut self, ui: &mut egui::Ui) {
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
                device::MIN_VOLTAGE_MV as f32 / 1000.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
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
        ui.horizontal(|ui| {
            if self.mode_on {
                if ui.button("Stop").clicked() {
                    self.cmd_tx.unbounded_send(OutboundFrame::Stop).ok();
                    self.mode_on = false;
                }
            } else if ui
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
                self.mode_on = true;
                self.mode_start_time = Some(ui.ctx().input(|ui| ui.time));
                self.voltage_points.clear();
                self.amperes_points.clear();
            }
            if ui
                .add_enabled(
                    has_live_voltage(self) && !self.mode_on,
                    egui::Button::new("Continue"),
                )
                .clicked()
            {
                self.cmd_tx
                    .unbounded_send(OutboundFrame::StartConstantVoltageCharge(
                        (self.charge_current * 1000.0) as u16,
                        (self.charge_voltage * 1000.0) as u16,
                        (self.charge_cutoff_current * 1000.0) as u16,
                    ))
                    .ok();
                self.mode_on = true;
                if self.mode_start_time.is_none() {
                    self.mode_start_time = Some(ui.ctx().input(|ui| ui.time));
                }
            }
        });
    }

    fn control_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Control");
        ui.label("Device Mode:");
        egui::ComboBox::from_id_salt("device_mode_selector")
            .selected_text(self.selected_device_mode.to_string())
            .show_ui(ui, |ui| {
                for mode in [
                    device::DeviceMode::DischargeConstantCurrent,
                    device::DeviceMode::DischargeConstantPower,
                    device::DeviceMode::ChargeConstantVoltage,
                ] {
                    ui.selectable_value(&mut self.selected_device_mode, mode, mode.to_string());
                }
            });
        match self.selected_device_mode {
            device::DeviceMode::DischargeConstantCurrent => {
                self.discharge_constant_current_ui(ui);
            }
            device::DeviceMode::DischargeConstantPower => {
                self.discharge_constant_power_ui(ui);
            }
            device::DeviceMode::ChargeConstantVoltage => {
                self.charge_constant_voltage_ui(ui);
            }
        }
    }

    fn plot_ui(&self, ui: &mut egui::Ui) {
        let label_formatter = |_s: &str, val: &PlotPoint| {
            format!(
                "{}: {:.3} V, {:.2} A",
                format_duration(val.x),
                val.y,
                self.live_current_ma as f64 / 1000.0
            )
        };

        let time_axis_formatter =
            |mark: egui_plot::GridMark, _range: &std::ops::RangeInclusive<f64>| {
                format_duration(mark.value)
            };

        Plot::new("live_data_plot")
            .legend(Legend::default())
            .label_formatter(label_formatter)
            .custom_x_axes(vec![
                AxisHints::new_x()
                    .label("Time")
                    .formatter(time_axis_formatter),
                AxisHints::new_x()
                    .label("Time")
                    .placement(VPlacement::Top)
                    .formatter(time_axis_formatter),
            ])
            .custom_y_axes(vec![
                AxisHints::new_y()
                    .label("Voltage (V) / Current (A)")
                    .placement(HPlacement::Left),
                AxisHints::new_y()
                    .label("Voltage (V) / Current (A)")
                    .placement(HPlacement::Right),
            ])
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

        egui::Panel::left("left_panel").show_inside(ui, |ui| {
            self.usb_ui(ui);
            ui.push_id("control_section", |ui| {
                if self.status == ConnectionStatus::Connected {
                    self.live_data_ui(ui);
                    self.control_ui(ui);
                }
            });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.plot_ui(ui);
        });
    }
}

fn format_duration(total_seconds: f64) -> String {
    let total_seconds = total_seconds as u64;
    let h = total_seconds / 3600;
    let m = (total_seconds % 3600) / 60;
    let s = total_seconds % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
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
