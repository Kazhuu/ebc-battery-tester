use crate::device;
use crate::device::OutboundFrame;
use crate::session::DeviceSession;

#[derive(Default)]
pub(crate) struct CalibrateWindow {
    pub(crate) open: bool,
    voltage_low: f32,
    voltage_high: f32,
    current_low: f32,
    current_high: f32,
}

impl CalibrateWindow {
    #[expect(clippy::too_many_lines)]
    pub(crate) fn ui(&mut self, session: &mut DeviceSession, ui: &egui::Ui) {
        if !self.open {
            return;
        }
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
                        egui::DragValue::new(&mut self.voltage_low)
                            .suffix(" V")
                            .range(0.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0)
                            .speed(0.001)
                            .max_decimals(3)
                            .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                    );
                    if ui.button("Calibrate").clicked() {
                        let mv = (self.voltage_low * 1000.0) as u16;
                        session.send_cmd(OutboundFrame::CalibrateVoltageLow(mv), ui.ctx());
                    }
                    ui.end_row();
                    ui.label("High (~4 V):");
                    ui.add(
                        egui::DragValue::new(&mut self.voltage_high)
                            .suffix(" V")
                            .range(0.0..=device::MAX_VOLTAGE_MV as f32 / 1000.0)
                            .speed(0.001)
                            .max_decimals(3)
                            .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                    );
                    if ui.button("Calibrate").clicked() {
                        let mv = (self.voltage_high * 1000.0) as u16;
                        session.send_cmd(OutboundFrame::CalibrateVoltageHigh(mv), ui.ctx());
                    }
                    ui.end_row();
                });
                ui.separator();
                ui.heading("Current");
                let discharge_active = session.mode_on
                    && matches!(
                        session.current_device_mode,
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
                        egui::DragValue::new(&mut self.current_low)
                            .suffix(" A")
                            .range(0.0..=device::MAX_CHARGE_CURRENT_MA as f32 / 1000.0)
                            .speed(0.001)
                            .max_decimals(3)
                            .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                    );
                    if ui
                        .add_enabled(discharge_active, egui::Button::new("Calibrate"))
                        .on_disabled_hover_text("Start a discharge constant current session first")
                        .clicked()
                    {
                        let ma = (self.current_low * 1000.0) as u16;
                        session.send_cmd(OutboundFrame::CalibrateCurrentLow(ma), ui.ctx());
                    }
                    ui.end_row();
                    ui.label("High (~2 A):");
                    ui.add(
                        egui::DragValue::new(&mut self.current_high)
                            .suffix(" A")
                            .range(0.0..=device::MAX_CHARGE_CURRENT_MA as f32 / 1000.0)
                            .speed(0.001)
                            .max_decimals(3)
                            .custom_parser(|s| s.replace(',', ".").parse::<f64>().ok()),
                    );
                    if ui
                        .add_enabled(discharge_active, egui::Button::new("Calibrate"))
                        .on_disabled_hover_text("Start a discharge constant current session first")
                        .clicked()
                    {
                        let ma = (self.current_high * 1000.0) as u16;
                        session.send_cmd(OutboundFrame::CalibrateCurrentHigh(ma), ui.ctx());
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
                            self.open = false;
                        }
                        if ui.button("OK").clicked() {
                            session.send_cmd(OutboundFrame::CalibrateConfirm, ui.ctx());
                            self.open = false;
                        }
                    });
                });
            });
    }
}
