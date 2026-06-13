# EBC-A20 Firmware Reverse Engineering Notes

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
