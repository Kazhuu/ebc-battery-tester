#[cfg(target_arch = "wasm32")]
#[path = "usb_wasm.rs"]
mod usb;

#[cfg(not(target_arch = "wasm32"))]
#[path = "usb_native.rs"]
mod usb;

mod app;
mod device;
pub use app::MainApp;
