#[cfg(target_arch = "wasm32")]
#[path = "usb_wasm.rs"]
mod usb;

#[cfg(not(target_arch = "wasm32"))]
#[path = "usb_native.rs"]
mod usb;

#[cfg(not(target_arch = "wasm32"))]
mod update_check;

mod app;
mod device;
mod export;
mod session;
mod ui;
pub use app::MainApp;
