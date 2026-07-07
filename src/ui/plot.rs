use crate::session::DeviceSession;
use crate::ui::format_duration;
use egui_plot::{AxisHints, HPlacement, Legend, Line, Plot, PlotPoint, PlotPoints, VPlacement};

pub(crate) fn ui(session: &DeviceSession, ui: &mut egui::Ui) {
    let label_formatter = |_s: &str, val: &PlotPoint| {
        format!(
            "{}: {:.3} V, {:.2} A",
            format_duration(val.x),
            val.y,
            session.live_current_ma as f64 / 1000.0
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
                Line::new("Voltage", PlotPoints::Borrowed(&session.voltage_points)).name("Voltage"),
            );
            plot_ui.line(
                Line::new("Current", PlotPoints::Borrowed(&session.amperes_points)).name("Current"),
            );
        });
}
