
#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Debug)]
pub enum DeviceCommand {
    Connect(usize),
    Disconnect,
}

#[derive(Debug)]
pub enum DeviceEvent {
    StatusChanged(ConnectionStatus),
    Frame(Vec<u8>),
}
