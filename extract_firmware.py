#!/usr/bin/env python3
"""
Extract EBC device firmware from a USB pcap captured during a firmware update.

The Windows software (eb.exe) uses the STM32 UART bootloader Write Memory
command (0x31 / complement 0xCE) over a CH340 USB-serial adapter to write
128-byte blocks to the device's flash, followed by a Read Memory verify pass
for each block.  This script:

  1. Reads any pcapng file captured from the CH340 (VID 0x1A86 PID 0x7523)
  2. Reassembles the serial TX byte stream from Bulk/Interrupt OUT packets
  3. Finds every Write Memory frame and validates its address checksum
  4. Reconstructs the flat binary image from the lowest written address

Usage:
    python3 extract_firmware.py <capture.pcap> [output.bin]
"""

import struct
import sys
from pathlib import Path


def load_pcapng_tx_stream(path: str) -> bytes:
    """Parse a pcapng file and return the reassembled serial TX byte stream
    from EP 0x02 OUT (Bulk or Interrupt) Submit packets."""
    with open(path, "rb") as f:
        data = f.read()

    offset = 0
    tx = bytearray()

    while offset < len(data) - 12:
        block_type, block_len = struct.unpack_from("<II", data, offset)
        if block_len < 12 or offset + block_len > len(data):
            break

        if block_type == 0x00000006:  # Enhanced Packet Block
            cap_len = struct.unpack_from("<I", data, offset + 20)[0]
            pkt = data[offset + 28 : offset + 28 + cap_len]

            if len(pkt) >= 64:
                event_type = chr(pkt[8])
                xfer_type  = pkt[9]   # 2=bulk, 3=interrupt
                epnum      = pkt[10]
                len_cap    = struct.unpack_from("<I", pkt, 36)[0]
                pkt_data   = pkt[64 : 64 + len_cap]

                is_out = not (epnum & 0x80)
                ep_num = epnum & 0x7F

                # Serial TX: EP2 OUT, Submit event (data lives in the submit)
                if ep_num == 2 and is_out and event_type == "S" and xfer_type in (2, 3) and len_cap > 0:
                    tx.extend(pkt_data)

        offset += block_len

    return bytes(tx)


def extract_write_memory_ops(tx: bytes) -> list[tuple[int, bytes]]:
    """Scan the TX byte stream for STM32 Write Memory frames.

    Frame layout (after the bootloader has already ACKed the command byte):
        0x31 0xCE            command + complement
        addr[3] addr[2] addr[1] addr[0]   big-endian flash address
        addr_cksum           XOR of the four address bytes
        N                    (block_len - 1)
        data[0..N]           N+1 firmware bytes
        data_cksum           N XOR data[0] XOR … XOR data[N]

    Returns a list of (address, data_bytes) pairs, sorted by address.
    """
    ops: list[tuple[int, bytes]] = []
    i = 0
    while i < len(tx) - 1:
        if tx[i] == 0x31 and tx[i + 1] == 0xCE:
            # Need at least: cmd(2) + addr+cksum(5) + N(1) + ≥1 data + cksum(1)
            if i + 9 > len(tx):
                i += 1
                continue

            addr = struct.unpack_from(">I", tx, i + 2)[0]
            addr_cksum = tx[i + 6]
            expected_addr_ck = tx[i+2] ^ tx[i+3] ^ tx[i+4] ^ tx[i+5]

            if addr_cksum != expected_addr_ck:
                i += 1
                continue

            n = tx[i + 7]
            length = n + 1
            data_start = i + 8
            data_end   = data_start + length

            if data_end >= len(tx):
                i += 1
                continue

            payload = tx[data_start:data_end]
            data_cksum = tx[data_end]

            # Verify data checksum: N XOR all data bytes
            computed = n
            for b in payload:
                computed ^= b

            if computed != data_cksum:
                i += 1
                continue

            ops.append((addr, bytes(payload)))
            # Skip past this entire frame to avoid false matches in data
            i = data_end + 1
        else:
            i += 1

    ops.sort(key=lambda x: x[0])
    return ops


def reconstruct_firmware(ops: list[tuple[int, bytes]]) -> tuple[int, bytes]:
    """Assemble flash writes into a flat binary.

    Returns (base_address, binary_image).
    Gaps between non-contiguous writes are filled with 0x45 (the device's
    erased-flash fill byte).
    """
    if not ops:
        raise ValueError("No Write Memory operations found in capture")

    base   = ops[0][0]
    end    = max(addr + len(data) for addr, data in ops)
    image  = bytearray(b"\x45" * (end - base))

    for addr, data in ops:
        off = addr - base
        image[off : off + len(data)] = data

    return base, bytes(image)


def main() -> None:
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <capture.pcap> [output.bin]")
        sys.exit(1)

    pcap_path = sys.argv[1]
    out_path  = sys.argv[2] if len(sys.argv) > 2 else "firmware_extracted.bin"

    print(f"Reading {pcap_path} ...")
    tx = load_pcapng_tx_stream(pcap_path)
    print(f"  Serial TX stream: {len(tx)} bytes")

    if not tx:
        print("ERROR: no EP2 OUT data found — wrong pcap or wrong device?")
        sys.exit(1)

    print("Scanning for Write Memory frames ...")
    ops = extract_write_memory_ops(tx)

    if not ops:
        print("ERROR: no valid Write Memory frames found")
        sys.exit(1)

    print(f"  Found {len(ops)} write operations")
    block_sizes = sorted({len(d) for _, d in ops})
    addrs = [a for a, _ in ops]
    print(f"  Block sizes: {block_sizes} bytes")
    print(f"  Flash range: 0x{min(addrs):08X} – 0x{max(addrs)+len(ops[-1][1])-1:08X}")

    # Check for gaps (non-contiguous writes)
    for (a1, d1), (a2, _) in zip(ops, ops[1:]):
        if a1 + len(d1) != a2:
            print(f"  WARNING: gap between 0x{a1+len(d1):08X} and 0x{a2:08X}")

    base, image = reconstruct_firmware(ops)
    total_bytes = sum(len(d) for _, d in ops)
    print(f"  Total written: {total_bytes} bytes ({total_bytes / 1024:.1f} KB)")
    print(f"  Image size:    {len(image)} bytes (base 0x{base:08X})")

    Path(out_path).write_bytes(image)
    print(f"\nFirmware written to: {out_path}")


if __name__ == "__main__":
    main()
