
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum DeviceMode {
    DischargeConstantCurrent,
    DischargeConstantPower,
    ChargeConstantVoltage,
}

impl std::fmt::Display for DeviceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceMode::DischargeConstantCurrent => write!(f, "Discharge Constant Current"),
            DeviceMode::DischargeConstantPower => write!(f, "Discharge Constant Power"),
            DeviceMode::ChargeConstantVoltage => write!(f, "Charge Constant Voltage"),
        }
    }
}

#[derive(Debug)]
pub enum DeviceCommand {
    // The usize is the index of the device to connect to, as returned by enumerate_devices.
    Connect(usize),
    Disconnect,
    Stop,
    Continue,
    // Start constant current discharge mode, with current in mA, cutoff voltage
    // in mV, and cutoff time in minutes. If cutoff time is 0, means indefinite.
    StartConstantCurrentDischarge(u16, u16, u16),
    // Start constant power discharge mode, with power in W, cutoff voltage in mV,
    // and cutoff time in minutes. If cutoff time is 0, means indefinite.
    StartConstantPowerDischarge(u16, u16, u16),
    // Start constant voltage charge mode, with charge current in mA, charge
    // voltage in mV and cutoff current in mA. The cutoff current is used to
    // determine when to stop charging.
    StartConstantVoltageCharge(u16, u16, u16),
    StopConstantCurrentDischarge,
}

#[derive(Debug)]
pub enum DeviceEvent {
    StatusChanged(ConnectionStatus),
    // Vec of available devices.
    DevicesUpdated(Vec<UsbDeviceInfo>),
    Frame(Vec<u8>),
}

// Start of Frame (SOF) and End of Frame (EOF) bytes.
const START_BYTE: u8 = 0xfa;
const END_BYTE: u8 = 0xf8;

enum CommmandType {
    Connect = 0x05,
    Disconnect = 0x06,
    Stop = 0x02,
    StartConstantCurrentDischarge = 0x01,
    StartConstantPowerDischarge = 0x11,
    StartConstantVoltageCharge = 0x21,
    // TODO: This seems to trigger discharge constant power???
    Continue = 0x18,
    StopConstantCurrentDischarge = 0x08,
}

// Encoding to prevent bytes > 240 in the byte stream, allowing 0xfa and 0xf8
// to be safely used as SOF and EOF markers.
fn encode_base240(value: u16) -> (u8, u8) {
    debug_assert!(value < 0xf0 * 0xf0 + 0xf0);
    let h = (value / 0xf0) as u8;
    let l = (value % 0xf0) as u8;
    (h, l)
}

fn decode_base240(h: u8, l: u8) -> u16 {
    0xf0 * h as u16 + l as u16
}

fn xor_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0, |acc, &b| acc ^ b)
}

fn build_frame(payload: [u8; 7]) -> [u8; 10] {
    let mut frame = [0u8; 10];
    frame[0] = START_BYTE;
    frame[1..8].copy_from_slice(&payload);
    frame[8] = xor_checksum(&payload);
    frame[9] = END_BYTE;
    frame
}

pub const MIN_DISCHARGE_CURRENT_MA: u16 = 10;
pub const MAX_DISCHARGE_CURRENT_MA: u16 = 20000;
pub const MIN_CHARGE_CURRENT_MA: u16 = 10;
pub const MAX_CHARGE_CURRENT_MA: u16 = 5000;
pub const MIN_CHARGE_CUTOFF_CURRENT_MA: u16 = 10;
pub const MAX_CHARGE_CUTOFF_CURRENT_MA: u16 = 9990;
pub const MIN_POWER_W: u16 = 1;
pub const MAX_POWER_W: u16 = 999;
pub const MIN_VOLTAGE_MV: u16 = 10;
pub const MAX_VOLTAGE_MV: u16 = 30000;
pub const MIN_CUTOFF_TIME_MIN: u16 = 0;
pub const MAX_CUTOFF_TIME_MIN: u16 = 999;
// Max minutes to wait between charge and discharge cycle.
pub const AUTO_MODE_TIME_MIN_MINS: u16 = 0;
pub const AUTO_MODE_TIME_MAX_MINS: u16 = 10;

// Send connect command to the device. This will display '-PC-' on the LCD screen.
pub fn connect_command() -> [u8; 10] {
    build_frame([CommmandType::Connect as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
}

// Send disconnect command to the device. After this '-PC-' disappears from LCD screen.
pub fn disconnect_command() -> [u8; 10] {
    build_frame([CommmandType::Disconnect as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
}

// Stop ongoing discharge or charge mode.
pub fn stop_command() -> [u8; 10] {
    build_frame([CommmandType::Stop as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
}

// TODO: Does not work yet. Seems to trigger discharge constant power???
pub fn continue_command() -> [u8; 10] {
    build_frame([CommmandType::Continue as u8, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00])
}

// TODO: Does not work yet.
pub fn stop_constant_current_discharge_command() -> [u8; 10] {
    build_frame([CommmandType::StopConstantCurrentDischarge as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
}

// Start constant current discharge mode with given discharge current in mA,
// cutoff voltage in mV, and cutoff time in minutes. If cutoff time is 0, means
// indefinite. The LCD screen will display 'DSC' mode. The cutoff current
// voltage values are quantized to 10mA and 10mV. This is because the device
// only allows setting the value in steps of 10 minimum. Maximum current is 20A,
// maximum voltage is 30V, and maximum cutoff time is 999 minutes. These are
// also same limits the device has.
pub fn start_constant_current_discharge_command(current_ma: u16, cutoff_mv: u16, cutoff_time_min: u16) -> [u8; 10] {
    if current_ma < MIN_DISCHARGE_CURRENT_MA || current_ma > MAX_DISCHARGE_CURRENT_MA {
        panic!("Current must be between {}mA and {}mA", MIN_DISCHARGE_CURRENT_MA, MAX_DISCHARGE_CURRENT_MA);
    }
    if cutoff_mv < MIN_VOLTAGE_MV || cutoff_mv > MAX_VOLTAGE_MV {
        panic!("Cutoff voltage must be between {}mV and {}mV", MIN_VOLTAGE_MV, MAX_VOLTAGE_MV);
    }
    if cutoff_time_min < MIN_CUTOFF_TIME_MIN || cutoff_time_min > MAX_CUTOFF_TIME_MIN {
        panic!("Cutoff time must be between {} and {} minutes", MIN_CUTOFF_TIME_MIN, MAX_CUTOFF_TIME_MIN);
    }
    let (current_h, current_l) = encode_base240(current_ma / 10);
    let (cutoff_h, cutoff_l) = encode_base240(cutoff_mv / 10);
    let (time_h, time_l) = encode_base240(cutoff_time_min);
    build_frame([
        CommmandType::StartConstantCurrentDischarge as u8,
        current_h,
        current_l,
        cutoff_h,
        cutoff_l,
        time_h,
        time_l,
    ])
}

pub fn start_constant_power_discharge_command(power_w: u16, cutoff_mv: u16, cutoff_time_min: u16) -> [u8; 10] {
    if power_w < MIN_POWER_W || power_w > MAX_POWER_W {
        panic!("Watts must be between {}W and {}W", MIN_POWER_W, MAX_POWER_W);
    }
    if cutoff_mv < MIN_VOLTAGE_MV || cutoff_mv > MAX_VOLTAGE_MV {
        panic!("Cutoff voltage must be between {}mV and {}mV", MIN_VOLTAGE_MV, MAX_VOLTAGE_MV);
    }
    if cutoff_time_min < MIN_CUTOFF_TIME_MIN || cutoff_time_min > MAX_CUTOFF_TIME_MIN {
        panic!("Cutoff time must be between {} and {} minutes", MIN_CUTOFF_TIME_MIN, MAX_CUTOFF_TIME_MIN);
    }
    let (power_h, power_l) = encode_base240(power_w);
    let (cutoff_h, cutoff_l) = encode_base240(cutoff_mv / 10);
    let (time_h, time_l) = encode_base240(cutoff_time_min);
    build_frame([
        CommmandType::StartConstantPowerDischarge as u8,
        power_h,
        power_l,
        cutoff_h,
        cutoff_l,
        time_h,
        time_l,
    ])
}

pub fn start_constant_voltage_charge_command(current_ma: u16, cutoff_mv: u16, cutoff_current_ma: u16) -> [u8; 10] {
    if current_ma < MIN_CHARGE_CURRENT_MA || current_ma > MAX_CHARGE_CURRENT_MA {
        panic!("Current must be between {}mA and {}mA", MIN_CHARGE_CURRENT_MA, MAX_CHARGE_CURRENT_MA);
    }
    if cutoff_mv < MIN_VOLTAGE_MV || cutoff_mv > MAX_VOLTAGE_MV {
        panic!("Cutoff voltage must be between {}mV and {}mV", MIN_VOLTAGE_MV, MAX_VOLTAGE_MV);
    }
    if cutoff_current_ma < MIN_CHARGE_CUTOFF_CURRENT_MA || cutoff_current_ma > MAX_CHARGE_CUTOFF_CURRENT_MA {
        panic!("Cutoff current must be between {}mA and {}mA", MIN_CHARGE_CUTOFF_CURRENT_MA, MAX_CHARGE_CUTOFF_CURRENT_MA);
    }
    let (current_h, current_l) = encode_base240(current_ma / 10);
    let (cutoff_h, cutoff_l) = encode_base240(cutoff_mv / 10);
    let (cutoff_current_h, cutoff_current_l) = encode_base240(cutoff_current_ma / 10);
    build_frame([
        CommmandType::StartConstantVoltageCharge as u8,
        current_h,
        current_l,
        cutoff_h,
        cutoff_l,
        cutoff_current_h,
        cutoff_current_l,
    ])
}
