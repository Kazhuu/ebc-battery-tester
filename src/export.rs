pub enum LogDirection {
    In,
    Out,
}

pub struct LogEntry {
    pub direction: LogDirection,
    pub label: String,
    pub timestamp: f64,
    pub raw_bytes: Vec<u8>,
}

pub fn format_log(entries: &[LogEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        let t = entry.timestamp as u64;
        let hours = t / 3600;
        let mins = (t % 3600) / 60;
        let secs = t % 60;
        let dir = match entry.direction {
            LogDirection::In => "IN ",
            LogDirection::Out => "OUT",
        };
        let hex: String = entry
            .raw_bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "{hours:02}:{mins:02}:{secs:02}  {dir}  {}\n         {hex}\n\n",
            entry.label
        ));
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_log_to_file(entries: &[LogEntry]) {
    let content = format_log(entries);
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Text", &["txt"])
        .set_file_name("frame_log.txt")
        .save_file()
    {
        std::fs::write(path, content).ok();
    }
}

#[cfg(target_arch = "wasm32")]
pub fn save_log_to_file(entries: &[LogEntry]) {
    use wasm_bindgen::JsCast as _;
    let content = format_log(entries);
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let array = js_sys::Array::new();
    array.push(&wasm_bindgen::JsValue::from_str(&content));
    let blob_opts = web_sys::BlobPropertyBag::new();
    blob_opts.set_type("text/plain");
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &blob_opts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    let Ok(anchor) = document.create_element("a") else {
        return;
    };
    let anchor: web_sys::HtmlAnchorElement = anchor.unchecked_into();
    anchor.set_href(&url);
    anchor.set_download("frame_log.txt");
    anchor.click();
    web_sys::Url::revoke_object_url(&url).ok();
}
