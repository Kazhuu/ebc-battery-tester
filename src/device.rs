use serde::{Deserialize, Serialize};

pub const VENDOR_ID: u16 = 0x1A86;

pub const MAX_FRAME_SIZE: usize = 19;

// Start of Frame (SOF) and End of Frame (EOF) bytes.
pub const START_BYTE: u8 = 0xfa;
pub const END_BYTE: u8 = 0xf8;

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
#[expect(unused)]
pub const AUTO_MODE_TIME_MIN_MINS: u16 = 0;
#[expect(unused)]
pub const AUTO_MODE_TIME_MAX_MINS: u16 = 10;

// ZKETECH EBC model codes sent from the device.
enum DeviceType {
    EbcA05 = 0x05,
    EbcA10H = 0x06,
    EbcA20 = 0x09,
}

fn get_device_model_name(device_type_code: u8) -> String {
    match device_type_code {
        x if x == DeviceType::EbcA05 as u8 => "EBC-A05".to_owned(),
        x if x == DeviceType::EbcA10H as u8 => "EBC-A10H".to_owned(),
        x if x == DeviceType::EbcA20 as u8 => "EBC-A20".to_owned(),
        _ => format!("Unknown ({device_type_code:#04x})"),
    }
}
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
            write!(
                f,
                "Unknown ({:04x}:{:04x})",
                self.vendor_id, self.product_id
            )
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

#[derive(Clone, Debug, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceMode {
    DischargeConstantCurrent,
    DischargeConstantPower,
    ChargeConstantVoltage,
}

impl std::fmt::Display for DeviceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DischargeConstantCurrent => write!(f, "Discharge Constant Current"),
            Self::DischargeConstantPower => write!(f, "Discharge Constant Power"),
            Self::ChargeConstantVoltage => write!(f, "Charge Constant Voltage"),
        }
    }
}

#[derive(Debug)]
pub enum OutboundFrame {
    // Send connect command to the device. This will display '-PC-' on the LCD
    // screen. The usize is the index of the device to connect to.
    Connect(usize),
    // Send disconnect command to the device. After this '-PC-' disappears from
    // LCD screen.
    Disconnect,
    // Stop ongoing discharge or charge mode.
    Stop,
    // TODO: Does not work yet. Seems to trigger discharge constant power???
    Continue,
    // Start constant current discharge mode with given discharge current in mA,
    // cutoff voltage in mV, and cutoff time in minutes. If cutoff time is 0,
    // means indefinite. The cutoff
    // current voltage values are quantized to 10mA and 10mV. This is because
    // the device only allows setting the value in steps of 10 minimum. Maximum
    // current is 20A, maximum voltage is 30V, and maximum cutoff time is 999
    // minutes. These are also same limits the device has.
    StartConstantCurrentDischarge(u16, u16, u16),
    // Start constant power discharge mode with given power in W, cutoff voltage
    // in mV, and cutoff time in minutes. If cutoff time is 0, means indefinite.
    // The cutoff voltage value is quantized to 10mV. This is
    // because the device only allows setting the value in steps of 10 minimum.
    // Maximum power is 200W, maximum voltage is 30V, and maximum cutoff time is
    // 999 minutes. These are also same limits the device has.
    StartConstantPowerDischarge(u16, u16, u16),
    // Start constant voltage charge mode with given charge current in mA,
    // charge voltage in mV and cutoff current in mA. The charge voltage and
    // current values are quantized to 10mV and 10mA. This is because the device
    // only allows setting the value in steps of 10 minimum. Maximum charge
    // current is 5A, maximum voltage is 30V, and maximum cutoff current is
    // 9990mA. These are also same limits the device has.
    StartConstantVoltageCharge(u16, u16, u16),
    StopConstantCurrentDischarge,
}

impl std::convert::From<OutboundFrame> for [u8; 10] {
    fn from(frame: OutboundFrame) -> Self {
        match frame {
            OutboundFrame::Connect(_) => connect_command(),
            OutboundFrame::Disconnect => disconnect_command(),
            OutboundFrame::Stop => stop_command(),
            OutboundFrame::Continue => continue_command(),
            OutboundFrame::StartConstantCurrentDischarge(
                current_ma,
                cutoff_mv,
                cutoff_time_min,
            ) => start_constant_current_discharge_command(current_ma, cutoff_mv, cutoff_time_min),
            OutboundFrame::StartConstantPowerDischarge(power_w, cutoff_mv, cutoff_time_min) => {
                start_constant_power_discharge_command(power_w, cutoff_mv, cutoff_time_min)
            }
            OutboundFrame::StartConstantVoltageCharge(
                current_ma,
                charge_voltage_mv,
                cutoff_current_ma,
            ) => start_constant_voltage_charge_command(
                current_ma,
                charge_voltage_mv,
                cutoff_current_ma,
            ),
            OutboundFrame::StopConstantCurrentDischarge => {
                stop_constant_current_discharge_command()
            }
        }
    }
}

enum CommmandType {
    StartConstantCurrentDischarge = 0x01,
    Stop = 0x02,
    Connect = 0x05,
    Disconnect = 0x06,
    StartConstantPowerDischarge = 0x11,
    StartConstantVoltageCharge = 0x21,
    // TODO: This seems to trigger discharge constant power???
    Continue = 0x18,
    // TODO: Does this work?
    StopConstantCurrentDischarge = 0x08,
    // TODO: Not tested.
    #[expect(unused)]
    AdjustDischargeConstantCurrent = 0x07,
    // TODO: Not tested.
    #[expect(unused)]
    ChargeTimeQuery = 0x0A,
}

enum StatusReportType {
    DischargeConstantCurrentOnReport = 0x0A,
    DischargeConstantCurrentOnFirmwareReport = 0x6E,
    DischargeConstantCurrentOffReport = 0x00,
    DischargeConstantCurrentOffFirmwareReport = 0x64,
    DischargeConstantCurrentEnd = 0x14,

    DischargeConstantPowerOnReport = 0x0B,
    DischargeConstantPowerOnFirmwareReport = 0x6F,
    DischargeConstantPowerOffReport = 0x01,
    DischargeConstantPowerOffFirmwareReport = 0x65,
    DischargeConstantPowerEnd = 0x15,

    ChargeConstantCurrentOnReport = 0x0C,
    ChargeConstantCurrentOnFirmwareReport = 0x70,
    ChargeConstantCurrentOffReport = 0x02,
    ChargeConstantCurrentOffFirmwareReport = 0x66,
    ChargeConstantCurrentEnd = 0x16,
}

#[derive(Clone, Debug)]
pub struct FirmwareReport {
    pub device_mode: DeviceMode,
    pub in_progress: bool,
    pub current_ma: u16,
    pub voltage_mv: u16,
    pub milli_ampere_hours: u16,
    #[expect(unused)]
    pub unknown: u16, // Always 0.
    pub firmware_version: String,
    // Calibration parameters, offset and gain maybe?
    #[expect(unused)]
    pub unknown1: u16, // Always 2988
    #[expect(unused)]
    pub unknown2: u16, // Always 2087
    pub device_type: String,
}

#[derive(Clone, Debug)]
pub struct ChargeReport {
    pub in_progress: bool,
    pub current_ma: u16,
    pub voltage_mv: u16,
    pub milli_ampere_hours: u16,
    #[expect(unused)]
    pub unknown: u16, // Always 0.
    #[expect(unused)]
    pub charge_current_ma: u16,
    #[expect(unused)]
    pub charge_voltage_mv: u16,
    #[expect(unused)]
    pub cutoff_current_ma: u16,
    pub device_type: String,
}

#[derive(Clone, Debug)]
pub struct DischargeConstantCurrentReport {
    pub in_progress: bool,
    pub current_ma: u16,
    pub voltage_mv: u16,
    pub milli_ampere_hours: u16,
    #[expect(unused)]
    pub unknown: u16, // Always 0.
    #[expect(unused)]
    pub discharge_current_ma: u16,
    #[expect(unused)]
    pub cutoff_voltage_mv: u16,
    #[expect(unused)]
    pub cutoff_time_min: u16,
    pub device_type: String,
}

#[derive(Clone, Debug)]
pub struct DischargeConstantPowerReport {
    pub in_progress: bool,
    pub current_ma: u16,
    pub voltage_mv: u16,
    pub milli_ampere_hours: u16,
    #[expect(unused)]
    pub unknown: u16, // Always 0.
    #[expect(unused)]
    pub discharge_power_w: u16,
    #[expect(unused)]
    pub cutoff_voltage_mv: u16,
    #[expect(unused)]
    pub cutoff_time_min: u16,
    pub device_type: String,
}

#[derive(Clone, Debug)]
pub enum InboundFrame {
    Firmware(FirmwareReport),
    DischargeConstantCurrent(DischargeConstantCurrentReport),
    DischargeConstantPower(DischargeConstantPowerReport),
    Charge(ChargeReport),
}

impl InboundFrame {
    fn construct_firmware_report(payload: &[u8]) -> Self {
        let command_byte = payload[0];
        let in_progress = command_byte
            == StatusReportType::ChargeConstantCurrentOnFirmwareReport as u8
            || command_byte == StatusReportType::DischargeConstantPowerOnFirmwareReport as u8
            || command_byte == StatusReportType::DischargeConstantCurrentOnFirmwareReport as u8;
        let device_mode = if command_byte
            == StatusReportType::ChargeConstantCurrentOnFirmwareReport as u8
            || command_byte == StatusReportType::ChargeConstantCurrentOffFirmwareReport as u8
        {
            DeviceMode::ChargeConstantVoltage
        } else if command_byte == StatusReportType::DischargeConstantPowerOnFirmwareReport as u8
            || command_byte == StatusReportType::DischargeConstantPowerOffFirmwareReport as u8
        {
            DeviceMode::DischargeConstantPower
        } else {
            DeviceMode::DischargeConstantCurrent
        };
        let version = decode_base240(payload[9], payload[10]);
        let major = version / 100;
        let minor = (version % 100) / 10;
        let patch = version % 10;

        Self::Firmware(FirmwareReport {
            device_mode,
            in_progress,
            current_ma: decode_base240(payload[1], payload[2]) * 10,
            voltage_mv: decode_base240(payload[3], payload[4]),
            milli_ampere_hours: decode_base240(payload[5], payload[6]),
            unknown: decode_base240(payload[7], payload[8]),
            firmware_version: format!("{major}.{minor}.{patch}"),
            unknown1: decode_base240(payload[11], payload[12]),
            unknown2: decode_base240(payload[13], payload[14]),
            device_type: get_device_model_name(payload[15]),
        })
    }

    fn construct_charge_report(payload: &[u8]) -> Self {
        let command_byte = payload[0];
        Self::Charge(ChargeReport {
            in_progress: command_byte == StatusReportType::ChargeConstantCurrentOnReport as u8,
            current_ma: decode_base240(payload[1], payload[2]) * 10,
            voltage_mv: decode_base240(payload[3], payload[4]),
            milli_ampere_hours: decode_base240(payload[5], payload[6]),
            unknown: decode_base240(payload[7], payload[8]),
            charge_current_ma: decode_base240(payload[9], payload[10]) * 10,
            charge_voltage_mv: decode_base240(payload[11], payload[12]),
            cutoff_current_ma: decode_base240(payload[13], payload[14]),
            device_type: get_device_model_name(payload[15]),
        })
    }

    fn construct_discharge_constant_current_report(payload: &[u8]) -> Self {
        let command_byte = payload[0];
        Self::DischargeConstantCurrent(DischargeConstantCurrentReport {
            in_progress: command_byte == StatusReportType::DischargeConstantCurrentOnReport as u8,
            current_ma: decode_base240(payload[1], payload[2]) * 10,
            voltage_mv: decode_base240(payload[3], payload[4]),
            milli_ampere_hours: decode_base240(payload[5], payload[6]),
            unknown: decode_base240(payload[7], payload[8]),
            discharge_current_ma: decode_base240(payload[9], payload[10]) * 10,
            cutoff_voltage_mv: decode_base240(payload[11], payload[12]),
            cutoff_time_min: decode_base240(payload[13], payload[14]),
            device_type: get_device_model_name(payload[15]),
        })
    }

    fn construct_discharge_constant_power_report(payload: &[u8]) -> Self {
        let command_byte = payload[0];
        Self::DischargeConstantPower(DischargeConstantPowerReport {
            in_progress: command_byte == StatusReportType::DischargeConstantPowerOnReport as u8,
            current_ma: decode_base240(payload[1], payload[2]) * 10,
            voltage_mv: decode_base240(payload[3], payload[4]),
            milli_ampere_hours: decode_base240(payload[5], payload[6]),
            unknown: decode_base240(payload[7], payload[8]),
            discharge_power_w: decode_base240(payload[9], payload[10]),
            cutoff_voltage_mv: decode_base240(payload[11], payload[12]),
            cutoff_time_min: decode_base240(payload[13], payload[14]),
            device_type: get_device_model_name(payload[15]),
        })
    }
}

impl TryFrom<&[u8]> for InboundFrame {
    type Error = String;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != MAX_FRAME_SIZE {
            return Err(format!(
                "Frame length mismatch: expected {}, got {}",
                MAX_FRAME_SIZE,
                value.len()
            ));
        }
        if value[0] != START_BYTE {
            return Err(format!(
                "Invalid start byte: expected {START_BYTE:#04x}, got {:#04x}",
                value[0]
            ));
        }
        if value[value.len() - 1] != END_BYTE {
            return Err(format!(
                "Invalid end byte: expected {END_BYTE:#04x}, got {:#04x}",
                value[value.len() - 1]
            ));
        }
        let payload = &value[1..value.len() - 2];
        let checksum = value[value.len() - 2];
        let calculated_checksum = xor_checksum(payload);
        // It seems there is a bug in the device firmware. When you discharge to
        // 3.3V with 1A. The checksum byte is wrong after about 2 mins of
        // discharging. If the checksum is ignored, the frame still has correct
        // data in it. This happens with firmware version 3.0.2 to me at least.
        // So we log the checksum error instead.
        if calculated_checksum != checksum {
            log::warn!(
                "Invalid checksum: expected {calculated_checksum:#04x}, got {checksum:#04x}"
            );
        }
        let command_byte = payload[0];
        match command_byte {
            // Charging, discharge (constant power and current) and idle
            // firmware reports have same frame structure. The difference is
            // that when charge is idle. It will send idle firmware report. When
            // charging, firmware report is sent for few seconds and not after
            // that. starting charge.
            x if x == StatusReportType::ChargeConstantCurrentOnFirmwareReport as u8
                || x == StatusReportType::ChargeConstantCurrentOffFirmwareReport as u8
                || x == StatusReportType::DischargeConstantPowerOnFirmwareReport as u8
                || x == StatusReportType::DischargeConstantPowerOffFirmwareReport as u8
                || x == StatusReportType::DischargeConstantCurrentOnFirmwareReport as u8
                || x == StatusReportType::DischargeConstantCurrentOffFirmwareReport as u8 =>
            {
                Ok(Self::construct_firmware_report(payload))
            }
            x if x == StatusReportType::ChargeConstantCurrentOnReport as u8
                || x == StatusReportType::ChargeConstantCurrentOffReport as u8
                || x == StatusReportType::ChargeConstantCurrentEnd as u8 =>
            {
                Ok(Self::construct_charge_report(payload))
            }
            x if x == StatusReportType::DischargeConstantCurrentOnReport as u8
                || x == StatusReportType::DischargeConstantCurrentOffReport as u8
                || x == StatusReportType::DischargeConstantCurrentEnd as u8 =>
            {
                Ok(Self::construct_discharge_constant_current_report(payload))
            }
            x if x == StatusReportType::DischargeConstantPowerOnReport as u8
                || x == StatusReportType::DischargeConstantPowerOffReport as u8
                || x == StatusReportType::DischargeConstantPowerEnd as u8 =>
            {
                Ok(Self::construct_discharge_constant_power_report(payload))
            }
            _ => Err(format!("Unknown command byte: {command_byte:#04x}")),
        }
    }
}

pub enum DeviceEvent {
    StatusChanged(ConnectionStatus),
    // Vec of available devices.
    DevicesUpdated(Vec<UsbDeviceInfo>),
    Frame(InboundFrame),
}

impl std::fmt::Debug for DeviceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StatusChanged(s) => f.debug_tuple("StatusChanged").field(s).finish(),
            Self::DevicesUpdated(d) => f.debug_tuple("DevicesUpdated").field(d).finish(),
            Self::Frame(frame) => {
                write!(f, "Frame({frame:?})")
            }
        }
    }
}

pub fn process_buffer(buf: &mut Vec<u8>) -> Vec<InboundFrame> {
    let mut frames = Vec::new();
    loop {
        if let Some(start) = buf.iter().position(|&b| b == START_BYTE) {
            if start > 0 {
                buf.drain(..start);
            }
        } else {
            buf.clear();
            break;
        }
        if let Some(end) = buf[1..].iter().position(|&b| b == END_BYTE) {
            let frame_end = end + 2; // +1 for slice offset, +1 for inclusive
            let frame = buf.drain(..frame_end).collect::<Vec<u8>>();
            match InboundFrame::try_from(frame.as_slice()) {
                Ok(f) => frames.push(f),
                Err(e) => log::warn!("Failed to parse frame: {e}"),
            }
        } else {
            break;
        }
    }
    frames
}

// Encoding to prevent bytes > 240 in the byte stream, allowing 0xfa and 0xf8
// to be safely used as SOF and EOF markers.
fn encode_base240(value: u16) -> (u8, u8) {
    debug_assert!(
        value < 0xf0 * 0xf0 + 0xf0,
        "Value too large to encode in base240: {value}"
    );
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

fn connect_command() -> [u8; 10] {
    build_frame([
        CommmandType::Connect as u8,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ])
}

fn disconnect_command() -> [u8; 10] {
    build_frame([
        CommmandType::Disconnect as u8,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ])
}

fn stop_command() -> [u8; 10] {
    build_frame([CommmandType::Stop as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
}

fn continue_command() -> [u8; 10] {
    build_frame([
        CommmandType::Continue as u8,
        0x00,
        0x03,
        0x00,
        0x00,
        0x00,
        0x00,
    ])
}

fn stop_constant_current_discharge_command() -> [u8; 10] {
    build_frame([
        CommmandType::StopConstantCurrentDischarge as u8,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ])
}

fn start_constant_current_discharge_command(
    current_ma: u16,
    cutoff_mv: u16,
    cutoff_time_min: u16,
) -> [u8; 10] {
    assert!(
        (MIN_DISCHARGE_CURRENT_MA..=MAX_DISCHARGE_CURRENT_MA).contains(&current_ma),
        "Current must be between {MIN_DISCHARGE_CURRENT_MA}mA and {MAX_DISCHARGE_CURRENT_MA}mA"
    );
    assert!(
        (MIN_VOLTAGE_MV..=MAX_VOLTAGE_MV).contains(&cutoff_mv),
        "Cutoff voltage must be between {MIN_VOLTAGE_MV}mV and {MAX_VOLTAGE_MV}mV"
    );
    assert!(
        (cutoff_time_min <= MAX_CUTOFF_TIME_MIN),
        "Cutoff time must be between 0 and {MAX_CUTOFF_TIME_MIN} minutes"
    );

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

fn start_constant_power_discharge_command(
    power_w: u16,
    cutoff_mv: u16,
    cutoff_time_min: u16,
) -> [u8; 10] {
    assert!(
        (MIN_POWER_W..=MAX_POWER_W).contains(&power_w),
        "Watts must be between {MIN_POWER_W}W and {MAX_POWER_W}W"
    );
    assert!(
        (MIN_VOLTAGE_MV..=MAX_VOLTAGE_MV).contains(&cutoff_mv),
        "Cutoff voltage must be between {MIN_VOLTAGE_MV}mV and {MAX_VOLTAGE_MV}mV"
    );
    assert!(
        (cutoff_time_min <= MAX_CUTOFF_TIME_MIN),
        "Cutoff time must be between 0 and {MAX_CUTOFF_TIME_MIN} minutes"
    );

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

fn start_constant_voltage_charge_command(
    current_ma: u16,
    charge_voltage_mv: u16,
    cutoff_current_ma: u16,
) -> [u8; 10] {
    assert!(
        (MIN_CHARGE_CURRENT_MA..=MAX_CHARGE_CURRENT_MA).contains(&current_ma),
        "Current must be between {MIN_CHARGE_CURRENT_MA}mA and {MAX_CHARGE_CURRENT_MA}mA"
    );
    assert!(
        (MIN_VOLTAGE_MV..=MAX_VOLTAGE_MV).contains(&charge_voltage_mv),
        "Charge voltage must be between {MIN_VOLTAGE_MV}mV and {MAX_VOLTAGE_MV}mV"
    );
    assert!(
        (MIN_CHARGE_CUTOFF_CURRENT_MA..=MAX_CHARGE_CUTOFF_CURRENT_MA).contains(&cutoff_current_ma),
        "Cutoff current must be between {MIN_CHARGE_CUTOFF_CURRENT_MA}mA and {MAX_CHARGE_CUTOFF_CURRENT_MA}mA"
    );

    let (current_h, current_l) = encode_base240(current_ma / 10);
    let (charge_voltage_h, charge_voltage_l) = encode_base240(charge_voltage_mv / 10);
    let (cutoff_current_h, cutoff_current_l) = encode_base240(cutoff_current_ma / 10);
    build_frame([
        CommmandType::StartConstantVoltageCharge as u8,
        current_h,
        current_l,
        charge_voltage_h,
        charge_voltage_l,
        cutoff_current_h,
        cutoff_current_l,
    ])
}
