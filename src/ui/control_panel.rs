use crate::device;
use crate::device::OutboundFrame;
use crate::session::DeviceSession;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct ControlPanel {
    selected_device_mode: device::DeviceMode,
    discharge_current: f32,
    discharge_cutoff_voltage: f32,
    discharge_watts: u16,
    discharge_time: u16,
    charge_current: f32,
    charge_voltage: f32,
    charge_cutoff_current: f32,
    #[serde(skip)]
    discharge_time_enabled: bool,
}

impl Default for ControlPanel {
    fn default() -> Self {
        Self {
            selected_device_mode: device::DeviceMode::DischargeConstantCurrent,
            discharge_current: 0.0,
            discharge_cutoff_voltage: 0.0,
            discharge_watts: 0,
            discharge_time: device::MIN_CUTOFF_TIME_MIN,
            charge_current: 0.0,
            charge_voltage: 0.0,
            charge_cutoff_current: 0.0,
            discharge_time_enabled: false,
        }
    }
}

impl ControlPanel {
    pub(crate) fn ui(&mut self, session: &mut DeviceSession, ui: &mut egui::Ui) {
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
                self.discharge_constant_current_buttons(session, ui);
            }
            device::DeviceMode::DischargeConstantPower => {
                self.discharge_constant_power_buttons(session, ui);
            }
            device::DeviceMode::ChargeConstantVoltage => {
                self.charge_constant_voltage_buttons(session, ui);
            }
        }
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

    fn discharge_constant_current_buttons(&self, session: &mut DeviceSession, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if session.mode_on {
                if ui.button("Stop").clicked() {
                    session.send_cmd(OutboundFrame::Stop, ui.ctx());
                    session.stop_mode(ui.ctx());
                }
            } else if ui
                .add_enabled(session.has_live_voltage(), egui::Button::new("Start"))
                .on_disabled_hover_text("Connect the device to a battery first")
                .clicked()
            {
                session.send_cmd(
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
                session.start_mode(ui.ctx());
            }
            if ui
                .add_enabled(
                    session.has_live_voltage() && !session.mode_on,
                    egui::Button::new("Continue"),
                )
                .clicked()
            {
                session.send_cmd(
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
                session.continue_mode(ui.ctx());
            }
            if ui
                .add_enabled(
                    session.has_live_voltage()
                        && session.mode_on
                        && session.current_device_mode
                            == Some(device::DeviceMode::DischargeConstantCurrent),
                    egui::Button::new("Adjust"),
                )
                .clicked()
            {
                session.send_cmd(
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

    fn discharge_constant_power_buttons(&self, session: &mut DeviceSession, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if session.mode_on {
                if ui.button("Stop").clicked() {
                    session.send_cmd(OutboundFrame::Stop, ui.ctx());
                    session.stop_mode(ui.ctx());
                }
            } else if ui
                .add_enabled(session.has_live_voltage(), egui::Button::new("Start"))
                .on_disabled_hover_text("Connect device to battery first")
                .clicked()
            {
                session.send_cmd(
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
                session.start_mode(ui.ctx());
            }
            if ui
                .add_enabled(
                    session.has_live_voltage() && !session.mode_on,
                    egui::Button::new("Continue"),
                )
                .clicked()
            {
                session.send_cmd(
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
                session.continue_mode(ui.ctx());
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

    fn charge_constant_voltage_buttons(&self, session: &mut DeviceSession, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if session.mode_on {
                if ui.button("Stop").clicked() {
                    session.send_cmd(OutboundFrame::Stop, ui.ctx());
                    session.stop_mode(ui.ctx());
                }
            } else if ui
                .add_enabled(session.has_live_voltage(), egui::Button::new("Start"))
                .on_disabled_hover_text("Connect device to battery first")
                .clicked()
            {
                session.send_cmd(
                    OutboundFrame::StartConstantVoltageCharge(
                        (self.charge_current * 1000.0) as u16,
                        (self.charge_voltage * 1000.0) as u16,
                        (self.charge_cutoff_current * 1000.0) as u16,
                    ),
                    ui.ctx(),
                );
                session.start_mode(ui.ctx());
            }
            if ui
                .add_enabled(
                    session.has_live_voltage() && !session.mode_on,
                    egui::Button::new("Continue"),
                )
                .clicked()
            {
                session.send_cmd(
                    OutboundFrame::ContinueConstantVoltageCharge(
                        (self.charge_current * 1000.0) as u16,
                        (self.charge_voltage * 1000.0) as u16,
                        (self.charge_cutoff_current * 1000.0) as u16,
                    ),
                    ui.ctx(),
                );
                session.continue_mode(ui.ctx());
            }
        });
    }
}
