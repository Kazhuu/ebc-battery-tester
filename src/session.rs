use crate::device;
use crate::export::{LogDirection, LogEntry};
use crate::usb;
use device::{ConnectionStatus, DeviceEvent, OutboundFrame};
use egui_plot::PlotPoint;
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

/// Live device connection: USB enumeration/connection state, telemetry
/// received from the device, and outgoing command dispatch.
pub(crate) struct DeviceSession {
    pub(crate) available_devices: Vec<device::UsbDeviceInfo>,
    pub(crate) selected_device_index: Option<usize>,
    pub(crate) cmd_tx: UnboundedSender<OutboundFrame>,
    event_rx: UnboundedReceiver<DeviceEvent>,
    pub(crate) event_tx: UnboundedSender<DeviceEvent>,
    pub(crate) status: ConnectionStatus,
    pub(crate) firmware_version: Option<String>,
    pub(crate) model_name: Option<String>,
    pub(crate) live_voltage_mv: u16,
    pub(crate) live_current_ma: u16,
    pub(crate) live_milli_ampere_hours: u16,
    pub(crate) voltage_points: Vec<PlotPoint>,
    pub(crate) amperes_points: Vec<PlotPoint>,
    pub(crate) current_device_mode: Option<device::DeviceMode>,
    pub(crate) mode_on: bool,
    pub(crate) log_entries: Vec<LogEntry>,
    last_timer_sync_min: u64,
    mode_start_time: f64,
    mode_accumulated_secs: f64,
}

impl Default for DeviceSession {
    fn default() -> Self {
        Self {
            available_devices: Default::default(),
            selected_device_index: None,
            cmd_tx: mpsc::unbounded::<OutboundFrame>().0,
            event_rx: mpsc::unbounded::<DeviceEvent>().1,
            event_tx: mpsc::unbounded::<DeviceEvent>().0,
            status: ConnectionStatus::Disconnected,
            firmware_version: None,
            model_name: None,
            live_voltage_mv: 0,
            live_current_ma: 0,
            live_milli_ampere_hours: 0,
            voltage_points: Vec::new(),
            amperes_points: Vec::new(),
            current_device_mode: None,
            mode_on: false,
            log_entries: Vec::new(),
            last_timer_sync_min: 0,
            mode_start_time: 0.0,
            mode_accumulated_secs: 0.0,
        }
    }
}

impl DeviceSession {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded::<OutboundFrame>();
        let (event_tx, event_rx) = mpsc::unbounded::<DeviceEvent>();
        usb::spawn_device_worker(ctx.clone(), cmd_rx, event_tx.clone());
        usb::enumerate_devices(event_tx.clone());
        Self {
            cmd_tx,
            event_rx,
            event_tx,
            ..Default::default()
        }
    }

    pub(crate) fn has_live_voltage(&self) -> bool {
        self.live_voltage_mv > 0
    }

    /// Shows elapsed time whether or not the current mode is running.
    pub(crate) fn displayed_elapsed_secs(&self, now: f64) -> f64 {
        if self.mode_on {
            self.elapsed_secs(now)
        } else {
            self.mode_accumulated_secs
        }
    }

    pub(crate) fn elapsed_secs(&self, now: f64) -> f64 {
        self.mode_accumulated_secs + (now - self.mode_start_time).max(0.0)
    }

    /// Marks a freshly started mode: resets the timer and clears the plot.
    pub(crate) fn start_mode(&mut self, ctx: &egui::Context) {
        self.mode_on = true;
        self.mode_start_time = ctx.input(|i| i.time);
        self.mode_accumulated_secs = 0.0;
        self.last_timer_sync_min = 0;
        self.voltage_points.clear();
        self.amperes_points.clear();
    }

    /// Marks a resumed mode: keeps the accumulated timer and plot history.
    pub(crate) fn continue_mode(&mut self, ctx: &egui::Context) {
        self.mode_on = true;
        self.mode_start_time = ctx.input(|i| i.time);
    }

    pub(crate) fn stop_mode(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.mode_accumulated_secs += now - self.mode_start_time;
        self.mode_start_time = 0.0;
        self.mode_on = false;
    }

    /// Sends a `TimerSync` command every minute when any mode is active. This
    /// is what the original Windows software is also doing, but the purpose
    /// of this is not confirmed.
    pub(crate) fn send_timer_sync_if_needed(&mut self, ctx: &egui::Context) {
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

    pub(crate) fn send_cmd(&mut self, frame: OutboundFrame, ctx: &egui::Context) {
        let raw_bytes: Vec<u8> = <[u8; device::OUTBOUND_FRAME_SIZE]>::from(frame.clone()).to_vec();
        self.log_entries.push(LogEntry {
            direction: LogDirection::Out,
            label: format!("{frame:?}"),
            timestamp: ctx.input(|i| i.time),
            raw_bytes,
        });
        self.cmd_tx.unbounded_send(frame).ok();
    }

    /// Stops the current mode and disconnects the device. Called on app
    /// exit so the device is not left in a running state.
    pub(crate) fn shutdown(&self) {
        self.cmd_tx.unbounded_send(OutboundFrame::Stop).ok();
        self.cmd_tx.unbounded_send(OutboundFrame::Disconnect).ok();
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

    pub(crate) fn consume_events(&mut self, ctx: &egui::Context) {
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
}
