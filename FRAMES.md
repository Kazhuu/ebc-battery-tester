# EBC-A20 Frame Reference

This document describes the binary protocol used by ZKETECH EBC battery testers.
It was reverse engineered from USB traffic captures. A useful starting point for
the initial investigation was this blog post, though it is incomplete and contains
some inaccuracies: <https://pop.fsck.pl/hardware/zketech-ebc-a20.html>

The device communicates over a **CH340 USB-serial adapter** (VID `0x1A86`,
PID `0x7523`) at **9600 baud, 8E1** (8 data bits, even parity, 1 stop bit).
There is no USB HID or custom USB class involved — it is plain serial over USB.

Some fields remain unknown and are noted as such.

## Table of Contents

- [EBC-A20 Frame Reference](#ebc-a20-frame-reference)
  - [Table of Contents](#table-of-contents)
  - [Frame structure](#frame-structure)
    - [Checksum](#checksum)
  - [Base240 encoding](#base240-encoding)
    - [Value scaling](#value-scaling)
  - [Outbound frames (host → device)](#outbound-frames-host--device)
    - [`0x05` Connect](#0x05-connect)
    - [`0x06` Disconnect](#0x06-disconnect)
    - [`0x02` Stop](#0x02-stop)
    - [`0x01` Start Constant Current Discharge](#0x01-start-constant-current-discharge)
    - [`0x07` Adjust Constant Current Discharge](#0x07-adjust-constant-current-discharge)
    - [`0x08` Resume Constant Current Discharge](#0x08-resume-constant-current-discharge)
    - [`0x11` Start Constant Power Discharge](#0x11-start-constant-power-discharge)
    - [`0x18` Resume Constant Power Discharge](#0x18-resume-constant-power-discharge)
    - [`0x21` Start Constant Voltage Charge](#0x21-start-constant-voltage-charge)
    - [`0x28` Resume Constant Voltage Charge](#0x28-resume-constant-voltage-charge)
    - [`0x0A` Timer sync](#0x0a-timer-sync)
    - [`0x09` Internal resistance test](#0x09-internal-resistance-test)
    - [`0x04` Calibration](#0x04-calibration)
  - [Inbound frames (device → host)](#inbound-frames-device--host)
    - [Firmware report](#firmware-report)
    - [Constant current discharge report](#constant-current-discharge-report)
    - [Constant power discharge report](#constant-power-discharge-report)
    - [Constant voltage charge report](#constant-voltage-charge-report)
  - [Device type codes](#device-type-codes)
  - [Quick reference — outbound frames](#quick-reference--outbound-frames)
  - [Quick reference — inbound frames](#quick-reference--inbound-frames)

## Frame structure

Every message in both directions uses the same wrapper:

```text
[0xfa] [payload...] [checksum] [0xf8]
```

| Byte(s) | Role | Value |
| --- | --- | --- |
| First byte | Start of frame (SOF) | Always `0xfa` |
| Middle bytes | Payload | Command-specific |
| Second-to-last byte | Checksum | XOR of all payload bytes |
| Last byte | End of frame (EOF) | Always `0xf8` |

- **Host → device** frames are always **10 bytes** (7-byte payload).
- **Device → host** frames are always **19 bytes** (16-byte payload).

### Checksum

The checksum is computed by XOR-ing every payload byte together (not including
the SOF, EOF, or the checksum byte itself):

```text
checksum = payload[0] ^ payload[1] ^ payload[2] ^ ... ^ payload[N]
```

**Example** — Connect command `fa 05 00 00 00 00 00 00 05 f8`:

```text
payload  = [0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
checksum = 0x05 ^ 0x00 ^ 0x00 ^ 0x00 ^ 0x00 ^ 0x00 ^ 0x00 = 0x05  ✓
```

**Example** — CC discharge 200 mA, 3.3 V cutoff `fa 01 00 14 01 5a 00 00 4b f8`:

```text
payload  = [0x01, 0x00, 0x14, 0x01, 0x5a, 0x00, 0x00]
checksum = 0x01 ^ 0x00 ^ 0x14 ^ 0x01 ^ 0x5a ^ 0x00 ^ 0x00 = 0x4b  ✓
```

## Base240 encoding

Numeric values that could be larger than one byte are encoded across two bytes
using base-240. This guarantees neither byte ever equals `0xfa` or `0xf8`,
which are reserved as SOF and EOF markers.

```text
encoded_high = value / 240
encoded_low  = value % 240

value = encoded_high * 240 + encoded_low
```

Both result bytes are always ≤ `0xef` (239), safely below the framing bytes.

### Value scaling

Most numeric fields are scaled before encoding to keep values in a usable
range:

| Field type | Scaling | Example |
| --- | --- | --- |
| Current | mA ÷ 10 | 2000 mA → encode 200 |
| Voltage | mV ÷ 10 | 4200 mV → encode 420 |
| Power | W (no scaling) | 5 W → encode 5 |
| Time | minutes (no scaling) | 90 min → encode 90 |
| Calibration values | full mV or mA | 1000 mV → encode 1000 |

## Outbound frames (host → device)

The host sends 10-byte command frames. The device does not send an explicit
acknowledgement — it responds by changing its behaviour and sending updated
status reports.

### `0x05` Connect

Opens a session with the device. While connected, the LCD shows `'-PC-'`
indicating the device is under PC control.

```text
fa 05 00 00 00 00 00 00 05 f8
```

### `0x06` Disconnect

Ends the session. Clears `'-PC-'` from the LCD and returns the device to
standalone operation.

```text
fa 06 00 00 00 00 00 00 06 f8
```

### `0x02` Stop

Stops any ongoing operation (discharge or charge). The device returns to idle.

```text
fa 02 00 00 00 00 00 00 02 f8
```

### `0x01` Start Constant Current Discharge

Begin discharging a battery at a fixed current until a cutoff voltage or time
limit is reached.

```text
[fa] [01] [current_h] [current_l] [cutoff_v_h] [cutoff_v_l] [time_h] [time_l] [checksum] [f8]
```

| Field | Encoding | Range |
| --- | --- | --- |
| `current` | mA ÷ 10, base240 | 10–20000 mA |
| `cutoff_v` | mV ÷ 10, base240 | 10–30000 mV |
| `time` | minutes, base240; 0 = no limit | 0–999 min |

```text
200 mA, 3.3 V cutoff, no time limit → fa 01 00 14 01 5a 00 00 4b f8
```

### `0x07` Adjust Constant Current Discharge

Change the discharge current while a CC discharge is already active. The
device adjusts immediately without stopping. Same payload layout as `0x01`.

```text
200 mA, 3.3 V cutoff, no time limit → fa 07 00 14 01 5a 00 00 48 f8
```

### `0x08` Resume Constant Current Discharge

Resume a CC discharge after it was stopped. Same payload layout as `0x01`.

```text
100 mA, 3.3 V cutoff, no time limit → fa 08 00 0a 01 5a 00 00 59 f8
```

### `0x11` Start Constant Power Discharge

Begin discharging at a fixed power level. The device adjusts current as the
battery voltage drops to maintain constant power.

```text
[fa] [11] [power_h] [power_l] [cutoff_v_h] [cutoff_v_l] [time_h] [time_l] [checksum] [f8]
```

| Field | Encoding | Range |
| --- | --- | --- |
| `power` | W, base240 | 1–999 W |
| `cutoff_v` | mV ÷ 10, base240 | 10–30000 mV |
| `time` | minutes, base240; 0 = no limit | 0–999 min |

### `0x18` Resume Constant Power Discharge

Resume a CP discharge after it was stopped. Same payload layout as `0x11`.

```text
1 W, 3.3 V cutoff, no time limit → fa 18 00 01 01 5a 00 00 42 f8
```

### `0x21` Start Constant Voltage Charge

Begin charging a battery at a target voltage. The charge current starts at the
set value and naturally tapers as the battery voltage approaches the target.
Charging stops when the current falls below the cutoff threshold.

```text
[fa] [21] [current_h] [current_l] [voltage_h] [voltage_l] [cutoff_i_h] [cutoff_i_l] [checksum] [f8]
```

| Field | Encoding | Range |
| --- | --- | --- |
| `current` | mA ÷ 10, base240 | 10–5000 mA |
| `voltage` | mV ÷ 10, base240 | 10–30000 mV |
| `cutoff_i` | mA ÷ 10, base240 | 10–9990 mA |

### `0x28` Resume Constant Voltage Charge

Resume a charge session after it was stopped. Same payload layout as `0x21`.

```text
3 A, 4.2 V, 0.1 A cutoff → fa 28 01 3c 01 b4 00 0a aa f8
```

### `0x0A` Timer sync

Observed to be sent once per minute during active charge or discharge
operations. Bytes 3–4 carry a base240-encoded minute counter starting from 1.
The exact purpose is not confirmed — it may synchronise the device's internal
timer, drive the elapsed-time display, or serve as a keep-alive. The device
does not send a dedicated reply beyond its normal periodic status reports.

```text
[fa] [0a] [00] [minutes_h] [minutes_l] [00] [00] [00] [checksum] [f8]
```

```text
1 min → fa 0a 00 01 00 00 00 00 0b f8
2 min → fa 0a 00 02 00 00 00 00 08 f8
3 min → fa 0a 00 03 00 00 00 00 09 f8
```

### `0x09` Internal resistance test

Triggers a brief discharge pulse at the specified current. The device measures
the battery voltage under load and replies with a single status report of
whichever discharge mode was last active (constant current or constant power).
The host computes internal resistance by comparing the open-circuit voltage
from the most recent prior report to the loaded voltage in this reply:
`R (mΩ) = (V_open − V_loaded) / I`.

```text
[fa] [09] [current_h] [current_l] [00] [00] [00] [00] [checksum] [f8]
```

| Current | Frame |
| --- | --- |
| 200 mA | `fa 09 00 14 00 00 00 00 1d f8` |
| 1000 mA | `fa 09 00 64 00 00 00 00 6d f8` |
| 2000 mA | `fa 09 00 c8 00 00 00 00 c1 f8` |

### `0x04` Calibration

Sets the voltage and current calibration reference points stored on the device.
To calibrate, apply a stable known reference (e.g. a precision power supply) to
the device input, then send the corresponding sub-command with the actual
reference value. According to the manual, the intended voltage reference points
are **1 V** (low) and **4 V** (high).

Each reference value is sent to the device individually as you click each
"Calibrate" button in the dialog. Sub-command `0x04` is sent when the dialog is
closed with OK. It is not yet confirmed whether the device discards the
previously sent reference values if the dialog is closed without sending this
confirm frame — this behaviour has not been tested.

```text
[fa] [04] [sub] [value_h] [value_l] [00] [00] [00] [checksum] [f8]
```

Values are in **full mV or mA** (not divided by 10), base240-encoded.

| `sub` | Operation |
| --- | --- |
| `0x00` | Set low voltage reference (mV) |
| `0x01` | Set high voltage reference (mV) |
| `0x02` | Set low current reference (mA) |
| `0x03` | Set high current reference (mA) |
| `0x04` | Confirm — write all four values to device storage |

```text
Low voltage  3747 mV → fa 04 00 0f 93 00 00 00 98 f8
High voltage 3758 mV → fa 04 01 0f 9e 00 00 00 94 f8
Low current  3000 mA → fa 04 02 0c 78 00 00 00 72 f8
High current 3001 mA → fa 04 03 0c 79 00 00 00 72 f8
Confirm              → fa 04 04 00 00 00 00 00 00 f8
```

*(The 3747/3758 mV values above were captured from a test session using a
battery rather than a precision reference — not a proper calibration.)*

## Inbound frames (device → host)

The device sends 19-byte status reports periodically. Which report type is sent
depends on the current operating mode. Payload byte `[0]` is the command byte
that identifies both the frame type and the current state (active, idle, or
finished).

### Firmware report

Sent for a few seconds immediately after the device connects to the PC,
regardless of the current operating mode. It carries the firmware version string
and two unknown constant values whose meaning has not been determined. After
those initial seconds the device switches to the mode-specific reports below and
does not send firmware reports again until the next connection.

| Cmd byte | Meaning |
| --- | --- |
| `0x64` | Last mode was CC discharge |
| `0x65` | Last mode was CP discharge |
| `0x66` | Last mode was CV charge |
| `0x6E` | Active — CC discharge |
| `0x6F` | Active — CP discharge |
| `0x70` | Active — CV charge |

Payload layout:

| Bytes | Field | Decoding |
| --- | --- | --- |
| `[0]` | Command byte | identifies mode |
| `[1–2]` | Current | base240 × 10 → mA |
| `[3–4]` | Voltage | base240 → mV |
| `[5–6]` | Capacity | base240 → mAh |
| `[7–8]` | Unknown | base240; always 0 in observed frames |
| `[9–10]` | Firmware version | base240 (e.g. 302 → "3.0.2") |
| `[11–12]` | Unknown | base240; always 2988 in observed frames |
| `[13–14]` | Unknown | base240; always 2087 in observed frames |
| `[15]` | Device type code | raw byte — see table below |

### Constant current discharge report

Sent continuously while a CC discharge is active, idle, or finished.

| Cmd byte | State |
| --- | --- |
| `0x00` | Idle |
| `0x0A` | Active |
| `0x14` | Finished — cutoff voltage or time limit reached |

Payload layout:

| Bytes | Field | Decoding |
| --- | --- | --- |
| `[0]` | Command byte | identifies state |
| `[1–2]` | Current | base240 × 10 → mA |
| `[3–4]` | Voltage | base240 → mV |
| `[5–6]` | Capacity | base240 → mAh |
| `[7–8]` | Unknown | base240; always 0 in observed frames |
| `[9–10]` | Set discharge current | base240 × 10 → mA |
| `[11–12]` | Set cutoff voltage | base240 → mV |
| `[13–14]` | Set cutoff time | base240 → minutes (0 = no limit) |
| `[15]` | Device type code | raw byte |

### Constant power discharge report

Sent continuously while a CP discharge is active, idle, or finished.

| Cmd byte | State |
| --- | --- |
| `0x01` | Idle |
| `0x0B` | Active |
| `0x15` | Finished — cutoff voltage or time limit reached |

Payload layout:

| Bytes | Field | Decoding |
| --- | --- | --- |
| `[0]` | Command byte | identifies state |
| `[1–2]` | Current | base240 × 10 → mA |
| `[3–4]` | Voltage | base240 → mV |
| `[5–6]` | Capacity | base240 → mAh |
| `[7–8]` | Unknown | base240; always 0 in observed frames |
| `[9–10]` | Set discharge power | base240 → W |
| `[11–12]` | Set cutoff voltage | base240 → mV |
| `[13–14]` | Set cutoff time | base240 → minutes (0 = no limit) |
| `[15]` | Device type code | raw byte |

### Constant voltage charge report

Sent continuously while a CV charge is active, idle, or finished.

| Cmd byte | State |
| --- | --- |
| `0x02` | Idle |
| `0x0C` | Active |
| `0x16` | Finished — cutoff current reached |

Payload layout:

| Bytes | Field | Decoding |
| --- | --- | --- |
| `[0]` | Command byte | identifies state |
| `[1–2]` | Current | base240 × 10 → mA |
| `[3–4]` | Voltage | base240 → mV |
| `[5–6]` | Capacity | base240 → mAh |
| `[7–8]` | Unknown | base240; always 0 in observed frames |
| `[9–10]` | Set charge current | base240 × 10 → mA |
| `[11–12]` | Set charge voltage | base240 → mV |
| `[13–14]` | Set cutoff current | base240 → mA |
| `[15]` | Device type code | raw byte |

## Device type codes

Byte `[15]` of every inbound frame identifies the connected model. Additional
codes exist in the Windows update tool but have not been observed over the wire.

| Code | Model |
| --- | --- |
| `0x05` | EBC-A05 |
| `0x06` | EBC-A10H |
| `0x09` | EBC-A20 |

## Quick reference — outbound frames

All outbound frames are 10 bytes: `[fa] [payload × 7] [chk] [f8]`.

| Cmd | Name | B1 | B2 | B3 | B4 | B5 | B6 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `05` | Connect | `00` | `00` | `00` | `00` | `00` | `00` |
| `06` | Disconnect | `00` | `00` | `00` | `00` | `00` | `00` |
| `02` | Stop | `00` | `00` | `00` | `00` | `00` | `00` |
| `01` | Start CC discharge | cur_h | cur_l | v_h | v_l | t_h | t_l |
| `07` | Adjust CC discharge | cur_h | cur_l | v_h | v_l | t_h | t_l |
| `08` | Resume CC discharge | cur_h | cur_l | v_h | v_l | t_h | t_l |
| `11` | Start CP discharge | pw_h | pw_l | v_h | v_l | t_h | t_l |
| `18` | Resume CP discharge | pw_h | pw_l | v_h | v_l | t_h | t_l |
| `21` | Start CV charge | cur_h | cur_l | v_h | v_l | ci_h | ci_l |
| `28` | Resume CV charge | cur_h | cur_l | v_h | v_l | ci_h | ci_l |
| `09` | Internal resistance | cur_h | cur_l | `00` | `00` | `00` | `00` |
| `0a` | Timer sync | `00` | min_h | min_l | `00` | `00` | `00` |
| `04` | Calibration | sub | val_h | val_l | `00` | `00` | `00` |

**Field key:**

- `cur` — current (mA÷10, base240)
- `v` — cutoff voltage (mV÷10, base240)
- `pw` — power (W, base240)
- `t` — time limit in minutes (base240; 0 = no limit)
- `ci` — cutoff current (mA÷10, base240)
- `min` — elapsed minutes (base240)
- `sub` — calibration sub-command (0x00–0x04)
- `val` — calibration value (full mV/mA, base240)

## Quick reference — inbound frames

All inbound frames are 19 bytes: `[fa] [payload × 16] [chk] [f8]`.

| Cmd byte(s) | Report type | P1–P2 | P3–P4 | P5–P6 | P7–P8 | P9–P10 | P11–P12 | P13–P14 | P15 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `64`/`65`/`66`/`6e`/`6f`/`70` | Firmware | cur | volt | mAh | ? | fw ver | ? | ? | dev |
| `00`/`0a`/`14` | CC discharge | cur | volt | mAh | ? | set cur | cutoff V | cutoff t | dev |
| `01`/`0b`/`15` | CP discharge | cur | volt | mAh | ? | set pw | cutoff V | cutoff t | dev |
| `02`/`0c`/`16` | CV charge | cur | volt | mAh | ? | set cur | set V | cutoff I | dev |

**Field key:**

- `cur` — current (base240 × 10 → mA)
- `volt` — voltage (base240 → mV)
- `mAh` — capacity (base240 → mAh)
- `fw ver` — firmware version (base240, e.g. 302 → "3.0.2")
- `set pw` — set discharge power (base240 → W)
- `cutoff t` — cutoff time (base240 → minutes)
- `dev` — device type code (raw byte)
- `?` — unknown; always 0 in observed frames except firmware report P11–P14 (constants 2988 and 2087, meaning unknown)

**Command bytes by mode:**

- CC discharge — `00` idle, `0a` active, `14` finished
- CP discharge — `01` idle, `0b` active, `15` finished
- CV charge — `02` idle, `0c` active, `16` finished
