
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbDeviceInfo {
    pub product_name: String,
    pub manufacturer_name: String,
    pub vendor_id: u16,
    pub product_id: u16,
}

impl std::fmt::Display for UsbDeviceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.product_name.is_empty() {
            write!(f, "Unknown ({:04x}:{:04x})", self.vendor_id, self.product_id)
        } else {
            write!(
                f,
                "{} ({:04x}:{:04x})",
                self.product_name, self.vendor_id, self.product_id
            )
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Debug)]
pub enum DeviceCommand {
    // The usize is the index of the device to connect to, as returned by enumerate_devices.
    Connect(usize),
    Disconnect,
}

#[derive(Debug)]
pub enum DeviceEvent {
    StatusChanged(ConnectionStatus),
    // Vec of available devices.
    DevicesUpdated(Vec<UsbDeviceInfo>),
    Frame(Vec<u8>),
}
