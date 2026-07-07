use crate::session::DeviceSession;
use crate::ui::format_duration;

pub(crate) fn ui(session: &DeviceSession, ui: &mut egui::Ui) {
    ui.separator();
    ui.heading("Live Data");
    if !session.has_live_voltage() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            "Connect the device to a battery",
        );
    }
    egui::Grid::new("measurements_grid").show(ui, |ui| {
        ui.label("Voltage:");
        ui.label(format!("{:.3} V", session.live_voltage_mv as f32 / 1000.0));
        ui.end_row();

        ui.label("Current:");
        ui.label(format!("{:.2} A", session.live_current_ma as f32 / 1000.0));
        ui.end_row();

        ui.label("Power:");
        ui.label(format!(
            "{:.2} W",
            (session.live_voltage_mv as f32 / 1000.0) * (session.live_current_ma as f32 / 1000.0)
        ));
        ui.end_row();

        ui.label("Energy:");
        ui.label(format!(
            "{:.0} mWh",
            (session.live_voltage_mv as f32) * (session.live_milli_ampere_hours as f32) / 1000.0
        ));
        ui.end_row();

        ui.label("Capacity:");
        ui.label(format!("{} mAh", session.live_milli_ampere_hours));
        ui.end_row();

        ui.label("Time:");
        ui.label(format_duration(
            session.displayed_elapsed_secs(ui.ctx().input(|i| i.time)),
        ));
        ui.end_row();

        ui.label("Mode:");
        if let Some(current_device_mode) = session.current_device_mode {
            ui.colored_label(
                if session.mode_on {
                    ui.visuals().warn_fg_color
                } else {
                    ui.visuals().text_color()
                },
                format!(
                    "{current_device_mode}{}",
                    if session.mode_on { " (On)" } else { " (Off)" }
                ),
            );
        } else {
            ui.label("--");
        }
        ui.end_row();

        ui.label("Model:");
        if let Some(model_name) = &session.model_name {
            ui.label(model_name);
        } else {
            ui.label("--");
        }
        ui.end_row();

        ui.label("Firmware:");
        if let Some(firmware_version) = &session.firmware_version {
            ui.label(format!("v{firmware_version}"));
        } else {
            ui.label("--");
        }
        ui.end_row();
    });
}
