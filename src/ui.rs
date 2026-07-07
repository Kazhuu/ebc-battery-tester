pub(crate) mod about_window;
pub(crate) mod calibrate_window;
pub(crate) mod control_panel;
pub(crate) mod live_data;
pub(crate) mod log_window;
pub(crate) mod plot;
pub(crate) mod usb_panel;

pub(crate) fn format_duration(total_seconds: f64) -> String {
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
