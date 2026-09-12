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

#[cfg(test)]
mod test {
    use super::format_duration;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0.0), "00:00");
        assert_eq!(format_duration(59.0), "00:59");
        assert_eq!(format_duration(60.0), "01:00");
        assert_eq!(format_duration(61.0), "01:01");
        assert_eq!(format_duration(3599.0), "59:59");
        assert_eq!(format_duration(3600.0), "1:00:00");
        assert_eq!(format_duration(3661.0), "1:01:01");
    }
}
