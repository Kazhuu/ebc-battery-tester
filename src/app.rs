use crate::device;
use crate::export::{LogDirection, LogEntry, save_log_to_file};
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
    selected_device_mode: device::DeviceMode,
    discharge_current: f32,
    discharge_cutoff_voltage: f32,
    discharge_watts: u16,
    discharge_time: u16,
    charge_current: f32,
    charge_voltage: f32,
    charge_cutoff_current: f32,

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
    current_device_mode: Option<device::DeviceMode>,
    #[serde(skip)]
    mode_on: bool,
    #[serde(skip)]
    discharge_time_enabled: bool,
    #[serde(skip)]
    open_calibration_window: bool,
    #[serde(skip)]
    open_about_window: bool,
    #[serde(skip)]
    open_log_window: bool,
    #[serde(skip)]
    log_entries: Vec<LogEntry>,
    #[serde(skip)]
    calibrate_voltage_low: f32,
    #[serde(skip)]
    calibrate_voltage_high: f32,
    #[serde(skip)]
    calibrate_current_low: f32,
    #[serde(skip)]
    calibrate_current_high: f32,
    #[serde(skip)]
    last_timer_sync_min: u64,
    #[serde(skip)]
    mode_start_time: f64,
    #[serde(skip)]
    mode_accumulated_secs: f64,
    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    update_check_state: crate::update_check::UpdateCheckState,
    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    update_check_rx: UnboundedReceiver<crate::update_check::UpdateCheckState>,
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
            discharge_time: device::MIN_CUTOFF_TIME_MIN,
            discharge_time_enabled: false,
            live_voltage_mv: 0,
            live_current_ma: 0,
            live_milli_ampere_hours: 0,
            voltage_points: Vec::new(),
            amperes_points: Vec::new(),
            open_calibration_window: false,
            open_about_window: false,
            open_log_window: false,
            log_entries: Vec::new(),
            calibrate_voltage_low: 0.0,
            calibrate_voltage_high: 0.0,
            calibrate_current_low: 0.0,
            calibrate_current_high: 0.0,
            last_timer_sync_min: 0,
            mode_start_time: 0.0,
            mode_accumulated_secs: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            update_check_state: crate::update_check::UpdateCheckState::Checking,
            #[cfg(not(target_arch = "wasm32"))]
            update_check_rx: mpsc::unbounded().1,
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (update_tx, update_rx) = mpsc::unbounded();
            crate::update_check::spawn_update_check(cc.egui_ctx.clone(), update_tx);
            app.update_check_rx = update_rx;
        }
        app
    }

    fn stop_mode(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.mode_accumulated_secs += now - self.mode_start_time;
        self.mode_start_time = 0.0;
        self.mode_on = false;
    }

    fn elapsed_secs(&self, now: f64) -> f64 {
        self.mode_accumulated_secs + (now - self.mode_start_time).max(0.0)
    }

    // Send a TimerSync command every minute when any mode is active. This is
    // what the original Windows software is also doing, but the purpose of this
    // is not confirmed.
    fn send_timer_sync_if_needed(&mut self, ctx: &egui::Context) {
        if !self.mode_on {
            return;
        }
        let now = ctx.input(|i| i.time);
        let elapsed_mins = (self.elapsed_secs(now) / 60.0) as u64;
        if elapsed_mins > self.last_timer_sync_min {
            self.last_timer_sync_min = elapsed_mins;
            self.send_cmd(OutboundFrame::TimerSync(elapsed_mins as u16), ctx);
        }
    }

    fn send_cmd(&mut self, frame: OutboundFrame, ctx: &egui::Context) {
        let raw_bytes: Vec<u8> = <[u8; device::OUTBOUND_FRAME_SIZE]>::from(frame.clone()).to_vec();
        self.log_entries.push(LogEntry {
            direction: LogDirection::Out,
            label: format!("{frame:?}"),
            timestamp: ctx.input(|i| i.time),
            raw_bytes,
        });
        self.cmd_tx.unbounded_send(frame).ok();
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
        let was_on = self.mode_on;
        self.mode_on = charge_report.in_progress;
        if was_on && !self.mode_on {
            self.stop_mode(ctx);
        }
        self.live_voltage_mv = charge_report.voltage_mv;
        self.live_current_ma = charge_report.current_ma;
        self.live_milli_ampere_hours = charge_report.milli_ampere_hours;
        self.model_name = Some(charge_report.device_type);
        if self.mode_on {
            let now = ctx.input(|i| i.time);
            let x = self.elapsed_secs(now);
            self.voltage_points
                .push(PlotPoint::new(x, self.live_voltage_mv as f64 / 1000.0));
            self.amperes_points
                .push(PlotPoint::new(x, self.live_current_ma as f64 / 1000.0));
        }
    }

    fn handle_discharge_constant_current_report(
        &mut self,
        discharge_report_struct: device::DischargeConstantCurrentReport,
        ctx: &egui::Context,
    ) {
        self.current_device_mode = Some(device::DeviceMode::DischargeConstantCurrent);
        let was_on = self.mode_on;
        self.mode_on = discharge_report_struct.in_progress;
        if was_on && !self.mode_on {
            self.stop_mode(ctx);
        }
        self.live_voltage_mv = discharge_report_struct.voltage_mv;
        self.live_current_ma = discharge_report_struct.current_ma;
        self.live_milli_ampere_hours = discharge_report_struct.milli_ampere_hours;
        self.model_name = Some(discharge_report_struct.device_type);
        if self.mode_on {
            let now = ctx.input(|i| i.time);
            let x = self.elapsed_secs(now);
            self.voltage_points
                .push(PlotPoint::new(x, self.live_voltage_mv as f64 / 1000.0));
            self.amperes_points
                .push(PlotPoint::new(x, self.live_current_ma as f64 / 1000.0));
        }
    }

    fn handle_discharge_constant_power_report(
        &mut self,
        discharge_report_struct: device::DischargeConstantPowerReport,
        ctx: &egui::Context,
    ) {
        self.current_device_mode = Some(device::DeviceMode::DischargeConstantPower);
        let was_on = self.mode_on;
        self.mode_on = discharge_report_struct.in_progress;
        if was_on && !self.mode_on {
            self.stop_mode(ctx);
        }
        self.live_voltage_mv = discharge_report_struct.voltage_mv;
        self.live_current_ma = discharge_report_struct.current_ma;
        self.live_milli_ampere_hours = discharge_report_struct.milli_ampere_hours;
        self.model_name = Some(discharge_report_struct.device_type);
        if self.mode_on {
            let now = ctx.input(|i| i.time);
            let x = self.elapsed_secs(now);
            self.voltage_points
                .push(PlotPoint::new(x, self.live_voltage_mv as f64 / 1000.0));
            self.amperes_points
                .push(PlotPoint::new(x, self.live_current_ma as f64 / 1000.0));
        }
    }

    fn consume_events(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        while let Ok(state) = self.update_check_rx.try_recv() {
            self.update_check_state = state;
        }
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                DeviceEvent::StatusChanged(status) => {
                    if matches!(status, ConnectionStatus::Error(_)) {
                        if self.mode_on {
                            self.stop_mode(ctx);
                        }
                        self.current_device_mode = None;
                    }
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
                DeviceEvent::Frame(frame, raw_bytes) => {
                    log::info!("Received frame: {frame:?}");
                    self.log_entries.push(LogEntry {
                        direction: LogDirection::In,
                        label: format!("{frame:?}"),
                        timestamp: ctx.input(|i| i.time),
                        raw_bytes,
                    });
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
                        self.send_cmd(OutboundFrame::Connect(idx), ui.ctx());
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.spinner();
                    ui.label("Connecting...");
                }
                ConnectionStatus::Connected => {
                    if ui.button("Disconnect").clicked() {
                        self.send_cmd(OutboundFrame::Stop, ui.ctx());
                        self.send_cmd(OutboundFrame::Disconnect, ui.ctx());
                    }
                }
                ConnectionStatus::Error(_) => {
                    if let Some(idx) = self.selected_device_index
                        && ui.button("Retry").clicked()
                    {
                        self.send_cmd(OutboundFrame::Connect(idx), ui.ctx());
                    }
                }
            }
        });
        if let ConnectionStatus::Error(msg) = &self.status {
            ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
        }
    }

    fn live_data_ui(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Live Data");
        if !has_live_voltage(self) {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Connect the device to a battery",
            );
        }
        egui::Grid::new("measurements_grid").show(ui, |ui| {
            ui.label("Voltage:");
            ui.label(format!("{:.3} V", self.live_voltage_mv as f32 / 1000.0));
            ui.end_row();

            ui.label("Current:");
            ui.label(format!("{:.2} A", self.live_current_ma as f32 / 1000.0));
            ui.end_row();

            ui.label("Power:");
            ui.label(format!(
                "{:.2} W",
                (self.live_voltage_mv as f32 / 1000.0) * (self.live_current_ma as f32 / 1000.0)
            ));
            ui.end_row();

            ui.label("Energy:");
            ui.label(format!(
                "{:.0} mWh",
                (self.live_voltage_mv as f32) * (self.live_milli_ampere_hours as f32) / 1000.0
            ));
            ui.end_row();

            ui.label("Capacity:");
            ui.label(format!("{} mAh", self.live_milli_ampere_hours));
            ui.end_row();

            ui.label("Time:");
            if self.mode_on {
                ui.label(format_duration(
                    self.elapsed_secs(ui.ctx().input(|i| i.time)),
                ));
            } else {
                ui.label(format_duration(self.mode_accumulated_secs));
            }
            ui.end_row();

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
            ui.end_row();

            ui.label("Model:");
            if let Some(model_name) = &self.model_name {
                ui.label(model_name);
            } else {
                ui.label("--");
            }
            ui.end_row();

            ui.label("Firmware:");
            if let Some(firmware_version) = &self.firmware_version {
                ui.label(format!("v{firmware_version}"));
            } else {
                ui.label("--");
            }
            ui.end_row();
        });
    }

    fn discharge_constant_current_params(&mut self, ui: &mut egui::Ui) {
        ui.label("Discharge Current:");
        ui.add(
            egui::DragValue::new(&mut self.discharge_current)
                .range(
                    device::MIN_DISCHARGE_CURRENT_MA as f32 / 1000.0
                        ..=device::MAX_DISCHARGE_CURRENT_MA as f32 / 1000.0,
                )
                .suffix(" A")
                .speed(0.005)
                .max_decimals(2)
                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
        );
        ui.end_row();
        ui.label("Cutoff Voltage:");
        ui.add(
            egui::DragValue::new(&mut self.discharge_cutoff_voltage)
                .range(
                    device::MIN_VOLTAGE_MV as f32 / 1000.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                )
                .suffix(" V")
                .speed(0.005)
                .max_decimals(2)
                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
        );
        ui.end_row();
        ui.checkbox(&mut self.discharge_time_enabled, "Cutoff Time:");
        if self.discharge_time_enabled {
            if self.discharge_time < device::MIN_CUTOFF_TIME_MIN + 1 {
                self.discharge_time = device::MIN_CUTOFF_TIME_MIN + 1;
            }
            ui.add(
                egui::DragValue::new(&mut self.discharge_time)
                    .range(device::MIN_CUTOFF_TIME_MIN + 1..=device::MAX_CUTOFF_TIME_MIN)
                    .suffix(" min")
                    .speed(1.0),
            );
        }
        ui.end_row();
    }

    fn discharge_constant_current_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.mode_on {
                if ui.button("Stop").clicked() {
                    self.send_cmd(OutboundFrame::Stop, ui.ctx());
                    self.stop_mode(ui.ctx());
                }
            } else if ui
                .add_enabled(has_live_voltage(self), egui::Button::new("Start"))
                .on_disabled_hover_text("Connect the device to a battery first")
                .clicked()
            {
                self.send_cmd(
                    OutboundFrame::StartConstantCurrentDischarge(
                        (self.discharge_current * 1000.0) as u16,
                        (self.discharge_cutoff_voltage * 1000.0) as u16,
                        if self.discharge_time_enabled {
                            self.discharge_time
                        } else {
                            device::MIN_CUTOFF_TIME_MIN
                        },
                    ),
                    ui.ctx(),
                );
                self.mode_on = true;
                self.mode_start_time = ui.ctx().input(|ui| ui.time);
                self.mode_accumulated_secs = 0.0;
                self.last_timer_sync_min = 0;
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
                self.send_cmd(
                    OutboundFrame::ContinueConstantCurrentDischarge(
                        (self.discharge_current * 1000.0) as u16,
                        (self.discharge_cutoff_voltage * 1000.0) as u16,
                        if self.discharge_time_enabled {
                            self.discharge_time
                        } else {
                            device::MIN_CUTOFF_TIME_MIN
                        },
                    ),
                    ui.ctx(),
                );
                self.mode_on = true;
                self.mode_start_time = ui.ctx().input(|ui| ui.time);
            }
            if ui
                .add_enabled(
                    has_live_voltage(self)
                        && self.mode_on
                        && self.current_device_mode
                            == Some(device::DeviceMode::DischargeConstantCurrent),
                    egui::Button::new("Adjust"),
                )
                .clicked()
            {
                self.send_cmd(
                    OutboundFrame::AdjustConstantCurrentDischarge(
                        (self.discharge_current * 1000.0) as u16,
                        (self.discharge_cutoff_voltage * 1000.0) as u16,
                        if self.discharge_time_enabled {
                            self.discharge_time
                        } else {
                            device::MIN_CUTOFF_TIME_MIN
                        },
                    ),
                    ui.ctx(),
                );
            }
        });
    }

    fn discharge_constant_power_params(&mut self, ui: &mut egui::Ui) {
        ui.label("Discharge Power:");
        ui.add(
            egui::DragValue::new(&mut self.discharge_watts)
                .range(device::MIN_POWER_W..=device::MAX_POWER_W)
                .suffix(" W")
                .speed(1.0),
        );
        ui.end_row();
        ui.label("Cutoff Voltage:");
        ui.add(
            egui::DragValue::new(&mut self.discharge_cutoff_voltage)
                .range(
                    device::MIN_VOLTAGE_MV as f32 / 1000.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                )
                .suffix(" V")
                .speed(0.005)
                .max_decimals(2)
                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
        );
        ui.end_row();
        ui.checkbox(&mut self.discharge_time_enabled, "Cutoff Time:");
        if self.discharge_time_enabled {
            if self.discharge_time < device::MIN_CUTOFF_TIME_MIN + 1 {
                self.discharge_time = device::MIN_CUTOFF_TIME_MIN + 1;
            }
            ui.add(
                egui::DragValue::new(&mut self.discharge_time)
                    .range(device::MIN_CUTOFF_TIME_MIN + 1..=device::MAX_CUTOFF_TIME_MIN)
                    .suffix(" min")
                    .speed(1.0),
            );
        }
        ui.end_row();
    }

    fn discharge_constant_power_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.mode_on {
                if ui.button("Stop").clicked() {
                    self.send_cmd(OutboundFrame::Stop, ui.ctx());
                    self.stop_mode(ui.ctx());
                }
            } else if ui
                .add_enabled(has_live_voltage(self), egui::Button::new("Start"))
                .on_disabled_hover_text("Connect device to battery first")
                .clicked()
            {
                self.send_cmd(
                    OutboundFrame::StartConstantPowerDischarge(
                        self.discharge_watts,
                        (self.discharge_cutoff_voltage * 1000.0) as u16,
                        if self.discharge_time_enabled {
                            self.discharge_time
                        } else {
                            device::MIN_CUTOFF_TIME_MIN
                        },
                    ),
                    ui.ctx(),
                );
                self.mode_on = true;
                self.mode_start_time = ui.ctx().input(|ui| ui.time);
                self.mode_accumulated_secs = 0.0;
                self.last_timer_sync_min = 0;
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
                self.send_cmd(
                    OutboundFrame::ContinueConstantPowerDischarge(
                        self.discharge_watts,
                        (self.discharge_cutoff_voltage * 1000.0) as u16,
                        if self.discharge_time_enabled {
                            self.discharge_time
                        } else {
                            device::MIN_CUTOFF_TIME_MIN
                        },
                    ),
                    ui.ctx(),
                );
                self.mode_on = true;
                self.mode_start_time = ui.ctx().input(|ui| ui.time);
            }
        });
    }

    fn charge_constant_voltage_params(&mut self, ui: &mut egui::Ui) {
        ui.label("Charge Current:");
        ui.add(
            egui::DragValue::new(&mut self.charge_current)
                .suffix(" A")
                .range(
                    device::MIN_CHARGE_CURRENT_MA as f32 / 1000.0
                        ..=device::MAX_CHARGE_CURRENT_MA as f32 / 1000.0,
                )
                .speed(0.005)
                .max_decimals(2)
                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
        );
        ui.end_row();
        ui.label("Charge Voltage:");
        ui.add(
            egui::DragValue::new(&mut self.charge_voltage)
                .range(
                    device::MIN_VOLTAGE_MV as f32 / 1000.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0,
                )
                .suffix(" V")
                .speed(0.005)
                .max_decimals(2)
                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
        );
        ui.end_row();
        ui.label("Cutoff Current:");
        ui.add(
            egui::DragValue::new(&mut self.charge_cutoff_current)
                .range(
                    device::MIN_CHARGE_CUTOFF_CURRENT_MA as f32 / 1000.0
                        ..=device::MAX_CHARGE_CUTOFF_CURRENT_MA as f32 / 1000.0,
                )
                .suffix(" A")
                .speed(0.005)
                .max_decimals(2)
                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
        );
        ui.end_row();
    }

    fn charge_constant_voltage_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.mode_on {
                if ui.button("Stop").clicked() {
                    self.send_cmd(OutboundFrame::Stop, ui.ctx());
                    self.stop_mode(ui.ctx());
                }
            } else if ui
                .add_enabled(has_live_voltage(self), egui::Button::new("Start"))
                .on_disabled_hover_text("Connect device to battery first")
                .clicked()
            {
                self.send_cmd(
                    OutboundFrame::StartConstantVoltageCharge(
                        (self.charge_current * 1000.0) as u16,
                        (self.charge_voltage * 1000.0) as u16,
                        (self.charge_cutoff_current * 1000.0) as u16,
                    ),
                    ui.ctx(),
                );
                self.mode_on = true;
                self.mode_start_time = ui.ctx().input(|ui| ui.time);
                self.mode_accumulated_secs = 0.0;
                self.last_timer_sync_min = 0;
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
                self.send_cmd(
                    OutboundFrame::ContinueConstantVoltageCharge(
                        (self.charge_current * 1000.0) as u16,
                        (self.charge_voltage * 1000.0) as u16,
                        (self.charge_cutoff_current * 1000.0) as u16,
                    ),
                    ui.ctx(),
                );
                self.mode_on = true;
                self.mode_start_time = ui.ctx().input(|ui| ui.time);
            }
        });
    }

    fn control_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Control");
        let selected_mode = self.selected_device_mode;
        egui::Grid::new("control_grid").show(ui, |ui| {
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
            ui.end_row();
            match selected_mode {
                device::DeviceMode::DischargeConstantCurrent => {
                    self.discharge_constant_current_params(ui);
                }
                device::DeviceMode::DischargeConstantPower => {
                    self.discharge_constant_power_params(ui);
                }
                device::DeviceMode::ChargeConstantVoltage => {
                    self.charge_constant_voltage_params(ui);
                }
            }
        });
        match selected_mode {
            device::DeviceMode::DischargeConstantCurrent => {
                self.discharge_constant_current_buttons(ui);
            }
            device::DeviceMode::DischargeConstantPower => {
                self.discharge_constant_power_buttons(ui);
            }
            device::DeviceMode::ChargeConstantVoltage => {
                self.charge_constant_voltage_buttons(ui);
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

    fn about_window_ui(&mut self, ui: &egui::Ui) {
        if self.open_about_window {
            egui::Window::new("About")
                .open(&mut self.open_about_window)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading("EBC Battery Tester");
                    ui.horizontal(|ui| {
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            match &self.update_check_state {
                                crate::update_check::UpdateCheckState::Checking => {
                                    ui.spinner();
                                    ui.weak("checking...");
                                }
                                crate::update_check::UpdateCheckState::UpToDate => {
                                    ui.label("(up to date)");
                                }
                                crate::update_check::UpdateCheckState::UpdateAvailable(tag) => {
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        format!("({tag} available)"),
                                    );
                                    ui.hyperlink_to(
                                        "Download",
                                        crate::update_check::RELEASES_PAGE_URL,
                                    );
                                }
                                crate::update_check::UpdateCheckState::Failed => {}
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        "Open source, cross-platform desktop and browser application to \
                         control the ZKetech EBC-A20 battery tester.",
                    );
                    ui.add_space(4.0);
                    ui.hyperlink_to(
                        "github.com/Kazhuu/ebc-battery-tester",
                        "https://github.com/Kazhuu/ebc-battery-tester",
                    );
                    ui.add_space(4.0);
                });
        }
    }

    fn log_window_ui(&mut self, ui: &egui::Ui) {
        if self.open_log_window {
            egui::Window::new("Frame Log")
                .open(&mut self.open_log_window)
                .resizable(true)
                .default_size([500.0, 600.0])
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} entries", self.log_entries.len()));
                        if ui.button("Clear").clicked() {
                            self.log_entries.clear();
                        }
                        if ui.button("Save").clicked() {
                            save_log_to_file(&self.log_entries);
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for entry in &self.log_entries {
                                let (prefix, color) = match entry.direction {
                                    LogDirection::In => {
                                        ("IN ", egui::Color32::from_rgb(100, 200, 100))
                                    }
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
                                        egui::RichText::new(format!(
                                            "{hours:02}:{mins:02}:{secs:02}"
                                        ))
                                        .monospace()
                                        .weak(),
                                    );
                                    ui.colored_label(
                                        color,
                                        egui::RichText::new(prefix).monospace(),
                                    );
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

    #[expect(clippy::too_many_lines)]
    fn calibrate_window_ui(&mut self, ui: &egui::Ui) {
        if self.open_calibration_window {
            egui::Window::new("Calibrate")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading("Voltage");
                    ui.label(
                        "Set a bench power supply to ~1 V (low) or ~4 V (high), \
                             connect it to the device input, then measure the exact \
                             voltage with a multimeter. Enter the measured value and \
                             press Calibrate.",
                    );
                    ui.add_space(4.0);
                    egui::Grid::new("calibrate_voltage_grid").show(ui, |ui| {
                        ui.label("Low (~1 V):");
                        ui.add(
                            egui::DragValue::new(&mut self.calibrate_voltage_low)
                                .suffix(" V")
                                .range(0.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0)
                                .speed(0.001)
                                .max_decimals(3)
                                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                        );
                        if ui.button("Calibrate").clicked() {
                            let mv = (self.calibrate_voltage_low * 1000.0) as u16;
                            self.send_cmd(OutboundFrame::CalibrateVoltageLow(mv), ui.ctx());
                        }
                        ui.end_row();
                        ui.label("High (~4 V):");
                        ui.add(
                            egui::DragValue::new(&mut self.calibrate_voltage_high)
                                .suffix(" V")
                                .range(0.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0)
                                .speed(0.001)
                                .max_decimals(3)
                                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                        );
                        if ui.button("Calibrate").clicked() {
                            let mv = (self.calibrate_voltage_high * 1000.0) as u16;
                            self.send_cmd(OutboundFrame::CalibrateVoltageHigh(mv), ui.ctx());
                        }
                        ui.end_row();
                    });
                    ui.separator();
                    ui.heading("Current");
                    let discharge_active = self.mode_on
                        && matches!(
                            self.current_device_mode,
                            Some(device::DeviceMode::DischargeConstantCurrent)
                        );
                    ui.label(
                        "Start a constant current discharge session at a known reference \
                             level (~0.5 A for low, ~2 A for high). Place a multimeter in \
                             series to measure the actual current. Enter the measured value \
                             and press Calibrate.",
                    );
                    ui.add_space(4.0);
                    egui::Grid::new("calibrate_current_grid").show(ui, |ui| {
                        ui.label("Low (~0.5 A):");
                        ui.add(
                            egui::DragValue::new(&mut self.calibrate_current_low)
                                .suffix(" A")
                                .range(0.0..=device::MAX_CHARGE_CURRENT_MA as f32 / 1000.0)
                                .speed(0.001)
                                .max_decimals(3)
                                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                        );
                        if ui
                            .add_enabled(discharge_active, egui::Button::new("Calibrate"))
                            .on_disabled_hover_text(
                                "Start a discharge constant current session first",
                            )
                            .clicked()
                        {
                            let ma = (self.calibrate_current_low * 1000.0) as u16;
                            self.send_cmd(OutboundFrame::CalibrateCurrentLow(ma), ui.ctx());
                        }
                        ui.end_row();
                        ui.label("High (~2 A):");
                        ui.add(
                            egui::DragValue::new(&mut self.calibrate_current_high)
                                .suffix(" A")
                                .range(0.0..=device::MAX_CHARGE_CURRENT_MA as f32 / 1000.0)
                                .speed(0.001)
                                .max_decimals(3)
                                .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                        );
                        if ui
                            .add_enabled(discharge_active, egui::Button::new("Calibrate"))
                            .on_disabled_hover_text(
                                "Start a discharge constant current session first",
                            )
                            .clicked()
                        {
                            let ma = (self.calibrate_current_high * 1000.0) as u16;
                            self.send_cmd(OutboundFrame::CalibrateCurrentHigh(ma), ui.ctx());
                        }
                        ui.end_row();
                    });
                    ui.separator();
                    ui.label(
                        "Each Calibrate button sends the reference value to the device \
                         immediately. Values are held in RAM and will be lost on the \
                         next power cycle. Press OK to write all values to permanent \
                         device storage.",
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                self.open_calibration_window = false;
                            }
                            if ui.button("OK").clicked() {
                                self.send_cmd(OutboundFrame::CalibrateConfirm, ui.ctx());
                                self.open_calibration_window = false;
                            }
                        });
                    });
                });
        }
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
        self.cmd_tx.unbounded_send(OutboundFrame::Stop).ok();
        self.cmd_tx.unbounded_send(OutboundFrame::Disconnect).ok();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.consume_events(ui.ctx());
        self.send_timer_sync_if_needed(ui.ctx());
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
                        self.status == ConnectionStatus::Connected && has_live_voltage(self),
                        egui::Button::new("Calibrate"),
                    )
                    .on_disabled_hover_text("Connect the device to a battery first")
                    .clicked()
                {
                    self.open_calibration_window = true;
                }
                ui.separator();
                if ui.button("Log").clicked() {
                    self.open_log_window = !self.open_log_window;
                }
                ui.separator();
                if ui.button("About").clicked() {
                    self.open_about_window = true;
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    match &self.update_check_state {
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
            self.about_window_ui(ui);
            self.calibrate_window_ui(ui);
            self.log_window_ui(ui);
            self.usb_ui(ui);
            ui.push_id("control_section", |ui| {
                if self.status == ConnectionStatus::Connected {
                    self.live_data_ui(ui);
                    self.control_ui(ui);
                }
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
