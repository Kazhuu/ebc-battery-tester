# EBC-A20 Firmware Reverse Engineering Notes

## Table of Contents

- [EBC-A20 Firmware Reverse Engineering Notes](#ebc-a20-firmware-reverse-engineering-notes)
  - [Table of Contents](#table-of-contents)
  - [Connection](#connection)
  - [Bootloader Protocol](#bootloader-protocol)
    - [Frame structure](#frame-structure)
    - [Commands](#commands)
    - [GET response (EBC-A20)](#get-response-ebc-a20)
  - [Flash Address Map](#flash-address-map)
  - [Firmware Dump via Read Memory](#firmware-dump-via-read-memory)
    - [Per-power-cycle read limit](#per-power-cycle-read-limit)
    - [Flash read protection](#flash-read-protection)
  - [Bootloader Region (`0x0000–0x8FFF`)](#bootloader-region-0x00000x8fff)
  - [Firmware Extraction from `eb.exe`](#firmware-extraction-from-ebexe)
  - [Firmware Extraction from USB pcap](#firmware-extraction-from-usb-pcap)
  - [Firmware Identity Check](#firmware-identity-check)
  - [Architecture Notes for Reverse Engineering](#architecture-notes-for-reverse-engineering)
  - [USB Traffic Capture](#usb-traffic-capture)
  - [Calibration Feature Notes](#calibration-feature-notes)
  - [Timer Sync Notes](#timer-sync-notes)
  - [Constant Current Charge Notes](#constant-current-charge-notes)
  - [Discharge Constant Current Notes](#discharge-constant-current-notes)
  - [Discharge Constant Power Notes](#discharge-constant-power-notes)
  - [Internal Resistance Test Notes](#internal-resistance-test-notes)

## Connection

The device communicates over a CH340 USB-serial adapter (VID `0x1A86`, PID `0x7523`).

- **Serial settings:** 9600 baud, 8E1 (8 data bits, even parity, 1 stop bit)
- **Port:** `/dev/ttyUSB0` on Linux

To enter bootloader mode:

1. Hold the **mode ON button** on the front panel
2. Flip the **power switch** on the back

The device stays in bootloader mode as long as it has power. Closing and re-opening the serial port (dropping DTR) does **not** reset it — external power keeps it running.

---

## Bootloader Protocol

The bootloader uses the **STM32 UART bootloader protocol** (AN3155) verbatim, which strongly suggests the MCU is an **STM8** — STMicro's 8-bit family shares the exact same protocol including sync byte, command set, ACK/NACK values, and frame structure.

Evidence for STM8:

- Flat address space starting at `0x0000` (not `0x08000000` like STM32)
- No ARM vector table at firmware start (`0x9000`)
- No ARM Thumb instruction patterns found in any dumped region
- Protocol match is exact: same sync, commands, parity, baud

### Frame structure

| Step | Bytes sent | Response |
| --- | --- | --- |
| Sync | `7F` | `79` (ACK) |
| Command | `cmd` `cmd^0xFF` | `79` (ACK) |
| Address | 4 bytes big-endian + XOR checksum | `79` (ACK) |
| Length (read) | `N-1` `(N-1)^0xFF` | `79` (ACK) + N bytes |
| Data (write) | `N-1` then N bytes + XOR checksum | `79` (ACK) |

### Commands

| Command | Code | Complement |
| --- | --- | --- |
| GET | `00` | `FF` |
| Read Memory | `11` | `EE` |
| Write Memory | `31` | `CE` |
| Erase | `43` | `BC` |

### GET response (EBC-A20)

```text
05 10 09 87 5A 10 4B 79
```

- Byte 2 (`0x09`) is the **device type code** — identifies this unit as EBC-A20
- `0x79` at end is the trailing ACK

Device type codes from `eb.exe` resource IDs:

| Code | Device |
| --- | --- |
| `0x06` | EBC-A10 |
| `0x09` | EBC-A20 |
| `0x24` | EBC-A40 |
| `0x33` | EBC-A20H |
| `0x65` | EBC-B20R |
| `0xBF` | EBC-A40L |
| `0xE7` | EBC-A20+ |

---

## Flash Address Map

```text
0x00000000 – 0x00008FFF   bootloader   (36 KB, heavily read-protected)
0x00009000 – 0x0000BD0F   firmware     (11,536 bytes = 45 × 256-byte blocks)
```

The address `0x9000` is where the Windows update tool (`eb.exe`) writes the firmware image.

---

## Firmware Dump via Read Memory

### Per-power-cycle read limit

The bootloader enforces a **read limit of approximately 6 × 256-byte blocks (1536 bytes) per power cycle**. After that it continues to ACK Read Memory commands but returns a repeated dummy 256-byte block instead of real flash data. The limit is not perfectly consistent — sometimes slightly more or fewer reads succeed before the dummy response kicks in.

To work around this: power cycle the device between read sessions and resume from the last known-good offset.

### Flash read protection

Even with correct power cycling, **four regions of the firmware are read-protected at the hardware level**. Each protected region returns a single repeated 256-byte dummy block for every read attempt, regardless of how many power cycles are performed:

| Firmware offset | Flash address | Size | Status |
| --- | --- | --- | --- |
| `0x0000–0x05FF` | `0x9000–0x95FF` | 1536 B | Readable |
| `0x0600–0x0AFF` | `0x9600–0x9AFF` | 1280 B | **Protected** |
| `0x0B00–0x11FF` | `0x9B00–0xA1FF` | 1792 B | Readable |
| `0x1200–0x16FF` | `0xA200–0xA6FF` | 1280 B | **Protected** |
| `0x1700–0x1EFF` | `0xA700–0xAEFF` | 2048 B | Readable |
| `0x1F00–0x23FF` | `0xAF00–0xB3FF` | 1280 B | **Protected** |
| `0x2400–0x24FF` | `0xB400–0xB4FF` | 256 B | Readable |
| `0x2500–0x29FF` | `0xB500–0xB9FF` | 1280 B | **Protected** |
| `0x2A00–0x2D0F` | `0xBA00–0xBD0F` | 784 B | Readable |

**6416 bytes readable (55.6%), 5120 bytes protected (44.4%).**

All readable bytes were verified to **match the firmware extracted from `eb.exe` exactly** — zero differences. This confirms the exe contains the correct and complete firmware image.

---

## Bootloader Region (`0x0000–0x8FFF`)

A partial dump covering `0x0000–0x1A00` shows the bootloader region is also heavily protected:

| Range | Content |
| --- | --- |
| `0x0000–0x00FF` | Real bootloader code |
| `0x0100–0x01FF` | Mostly erased flash |
| `0x0200–0x06FF` | Protected (dummy block) |
| `0x0700–0x07FF` | Real bootloader code |
| `0x0800–0x19FF` | Protected (dummy block = lookup table) |

The protected regions return two distinct dummy blocks, suggesting the hardware returns a specific pre-selected value rather than zeroes or erased bytes.

The dump confirms the MCU is **not ARM**: no valid ARM Cortex-M vector table at `0x0000`, no BX LR / PUSH LR Thumb patterns anywhere in readable regions.

---

## Firmware Extraction from `eb.exe`

The Windows update tool embeds all device firmware images as **PE custom resources** in the `.rsrc` section. Each resource ID matches the device type code from the bootloader GET response.

```bash
python3 extract_firmware_from_exe.py eb.exe fw_out/
```

The EBC-A20 firmware is **resource ID 9** at PE file offset `0x145308`, size 11536 bytes. The bytes are written verbatim to flash — no encoding, no compression.

Output: `fw_out/firmware_id9_EBC-A20.bin`
SHA-256: `30bf63bc6412fa671561e739c79c2fe85bab2fc77ccce9005955f95baa6d3d37`

---

## Firmware Extraction from USB pcap

Capture USB traffic during a firmware update with Wireshark (filter: `usb.idVendor == 0x1a86`). The Windows tool writes 128 bytes per block using Write Memory frames.

```bash
python3 extract_firmware.py firmware-update.pcap firmware_extracted.bin
```

The script filters **EP2 OUT Submit** packets from the CH340, scans for `31 CE` (Write Memory command + complement), validates address and data checksums, and reconstructs the flat binary.

The pcap-extracted firmware is **byte-for-byte identical** to `fw_out/firmware_id9_EBC-A20.bin`.

---

## Firmware Identity Check

The Windows software checks the device version before updating by reading 2 bytes from address `0x00004002` (in the bootloader region):

- Returns `45 45` → device has no firmware or erased region
- The address is in the bootloader's address space, not the firmware region

---

## Architecture Notes for Reverse Engineering

The firmware at `0x9000` starts with `C7 45 F9 EF` repeated — not an ARM vector table. This is direct executable code for the MCU.

Recommended tools:

- **Ghidra** with STM8 processor module: [github.com/SergeyLys/ghidra_8051](https://github.com/SergeyLys/ghidra_8051)
- **radare2**: `r2 -a stm8 -b 8 -m 0x9000 fw_out/firmware_id9_EBC-A20.bin`

Set the base address to `0x9000` when loading the firmware binary, as that is where it is mapped in the device's flat address space.

The firmware binary from `eb.exe` is authoritative — the readable dump regions match it exactly and the protected regions cannot be extracted via the bootloader interface.

---

## USB Traffic Capture

To capture new application-level frames, record USB traffic with tshark while
the device is connected via the CH340:

```bash
tshark -i usbmon1 -w output.pcap
```

Then filter by device address in Wireshark (find the address first with `lsusb`):

```text
usb.device_address == 11
```

---

## Calibration Feature Notes

The calibration dialog has four reference point fields: low voltage, high
voltage, low current and high current. According to the manual the voltage
references should be 1 V and 4 V. Each field has its own calibrate button, and
pressing OK at the end closes the dialog.

All calibration frames use command byte `0x04` and follow the same 10-byte
frame structure as other commands. The sub-command byte in the second position
selects the operation:

| Sub-cmd | Operation             | Data    |
|---------|-----------------------|---------|
| `0x00`  | Set low voltage ref   | full mV |
| `0x01`  | Set high voltage ref  | full mV |
| `0x02`  | Set low current ref   | full mA |
| `0x03`  | Set high current ref  | full mA |
| `0x04`  | Confirm/close dialog  | —       |

The value is base240-encoded in bytes 3–4, using full mV or mA instead of the
mV/10 and mA/10 scaling used by other commands.

```text
[0xfa] [0x04] [sub] [value_h] [value_l] [0x00] [0x00] [0x00] [checksum] [0xf8]
```

Captured frames:

```text
Low voltage  3747mV → fa 04 00 0f 93 00 00 00 98 f8
High voltage 3758mV → fa 04 01 0f 9e 00 00 00 94 f8
Low current  3000mA → fa 04 02 0c 78 00 00 00 72 f8
High current 3001mA → fa 04 03 0c 79 00 00 00 72 f8
Confirm             → fa 04 04 00 00 00 00 00 00 f8
```

Sub-commands `0x00`–`0x03` write reference values to device RAM only. They take
effect immediately but are lost on the next power cycle. Sub-command `0x04`
(Confirm) commits all four values to non-volatile storage. Cancelling the dialog
without sending `0x04` leaves any already-sent values active in RAM until the
next power cycle, after which the previous stored calibration is restored.

---

## Timer Sync Notes

Both charge and discharge modes send an elapsed-minutes counter to the device
once per minute using command `0x0A`. The minute count is base240-encoded in
payload bytes 1–2. The device does not respond beyond its normal report frames.

```text
1 min → fa 0a 00 01 00 00 00 00 0b f8
2 min → fa 0a 00 02 00 00 00 00 08 f8
3 min → fa 0a 00 03 00 00 00 00 09 f8
```

---

## Constant Current Charge Notes

Settings cannot be adjusted while charging is active.

Resume charge with 3 A, 4.2 V, cutoff 0.1 A uses command `0x28`:

```text
fa 28 01 3c 01 b4 00 0a aa f8
```

---

## Discharge Constant Current Notes

Settings can be adjusted on the fly using command `0x07`
(`AdjustDischargeConstantCurrent`). Encoding is the same as the start command:
current in mA/10, cutoff voltage in mV/10, time in minutes, all base240.

```text
200mA, 3.3V cutoff, no time limit → fa 07 00 14 01 5a 00 00 48 f8
```

Resume after stop uses command `0x08`. Despite the current
`StopConstantCurrentDischarge` label in the code, the captured frame includes
current, cutoff voltage and time parameters — it behaves like a resume/continue
rather than a plain stop.

```text
100mA, 3.3V cutoff, no time limit → fa 08 00 0a 01 5a 00 00 59 f8
```

---

## Discharge Constant Power Notes

Settings cannot be adjusted on the fly in this mode. Resume after stop uses
command `0x18` (`Continue`) with power in W, cutoff voltage in mV/10, and time
in minutes. The existing `continue_command()` in the code sends hardcoded zeros
for these parameters and is likely wrong.

```text
1W, 3.3V cutoff, no time limit → fa 18 00 01 01 5a 00 00 42 f8
```

---

## Internal Resistance Test Notes

The Windows software has a dialog where the user inputs a test current in mA
and presses Test. Command `0x09` triggers a brief discharge at that current.
The current is encoded the same way as discharge commands: mA/10, base240.

```text
 200mA → fa 09 00 14 00 00 00 00 1d f8
1000mA → fa 09 00 64 00 00 00 00 6d f8
2000mA → fa 09 00 c8 00 00 00 00 c1 f8
```

The device responds with a discharge off-report containing the battery voltage
measured under load, then the fan spins briefly.

```text
fa 01 00 00 0f 93 00 09 00 00 00 01 01 5a 00 00 09 c7 f8
→ voltage=3747mV, current=0mA, mAh=9, power=1W, cutoff=3300mV
```

The Windows software calculates internal resistance from two consecutive voltage
readings: `R = (V_open_circuit − V_under_load) / I`. The pre-test open-circuit
voltage comes from a prior firmware report; the loaded voltage comes from this
response. A result of 0 mΩ means the voltage drop was below measurement
resolution — either a very low impedance battery or the pulse was too short to
produce a measurable ΔV.
