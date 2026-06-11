#!/usr/bin/env python3
"""
Extract EBC device firmware binaries from the ZKETECH Windows software.

The exe embeds one firmware blob per supported device type as a PE CUSTOM resource.
The blobs are raw flash images written verbatim to the device starting at address
0x00009000 (the first 36 KB of flash is the bootloader).  Resource ID matches the
device type byte returned by the bootloader's GET command (e.g. ID 9 = 0x09 = EBC-A20).

Usage:
    python3 extract_firmware_from_exe.py [eb.exe] [output_dir]

Outputs one file per resource: firmware_id<N>.bin
"""

import struct
import sys
from pathlib import Path

# Known device type codes (bootloader GET response byte 3).
# Add more as they are identified from real devices.
DEVICE_NAMES: dict[int, str] = {
    0x06: "EBC-A10",
    0x09: "EBC-A20",
    0x24: "EBC-A40",
    0x33: "EBC-A20H",
    0x65: "EBC-B20R",
    0xBF: "EBC-A40L",
    0xE7: "EBC-A20+",
    0x10F: "unknown_0x10F",
    0x11B: "unknown_0x11B",
    0x11C: "unknown_0x11C",
    0x187: "unknown_0x187",
    0x188: "unknown_0x188",
}

FLASH_BASE = 0x00009000  # firmware start address (after 36 KB bootloader)


# ---------------------------------------------------------------------------
# Minimal PE parser
# ---------------------------------------------------------------------------


def rva_to_file_offset(rva: int, sections: list[tuple[int, int, int, int]]) -> int:
    """Convert a Relative Virtual Address to a raw file offset using the section table.
    sections is a list of (virt_addr, virt_size, raw_offset, raw_size).
    """
    for va, vsz, raw, rsz in sections:
        if va <= rva < va + max(vsz, rsz):
            return raw + (rva - va)
    raise ValueError(f"RVA 0x{rva:08X} not found in any section")


def parse_resource_directory(
    data: bytes,
    rsrc_file_off: int,
    rsrc_rva: int,
    dir_offset: int,
    level: int,
    type_id: int | None,
    name_id: int | None,
) -> list[tuple[int, int, int, int]]:
    """Recursively walk the PE resource directory tree.

    PE resource trees have three levels:
      0 — type  (e.g. 0x0804 for custom resources)
      1 — name/ID  (the ID we care about: 6, 9, 36 …)
      2 — language  (leaf)

    Returns a list of (type_id, name_id, data_rva, data_size) for every leaf.
    """
    results = []
    base = rsrc_file_off + dir_offset
    named_count, id_count = struct.unpack_from("<HH", data, base + 12)

    for i in range(named_count + id_count):
        entry_off = base + 16 + i * 8
        name_or_id, offset_or_data = struct.unpack_from("<II", data, entry_off)

        is_named = bool(name_or_id & 0x80000000)
        entry_id = name_or_id & 0x7FFFFFFF
        is_subdir = bool(offset_or_data & 0x80000000)
        child_off = offset_or_data & 0x7FFFFFFF

        if is_subdir:
            if level == 0:
                # This entry IS the type; record it and descend
                next_type = None if is_named else entry_id
                results.extend(
                    parse_resource_directory(
                        data, rsrc_file_off, rsrc_rva, child_off, 1, next_type, None
                    )
                )
            elif level == 1:
                # This entry IS the name/ID we want; record it and descend
                next_name = None if is_named else entry_id
                results.extend(
                    parse_resource_directory(
                        data, rsrc_file_off, rsrc_rva, child_off, 2, type_id, next_name
                    )
                )
            else:
                results.extend(
                    parse_resource_directory(
                        data,
                        rsrc_file_off,
                        rsrc_rva,
                        child_off,
                        level + 1,
                        type_id,
                        name_id,
                    )
                )
        else:
            # Leaf: IMAGE_RESOURCE_DATA_ENTRY (offset to RVA + size)
            leaf = rsrc_file_off + child_off
            data_rva, data_size = struct.unpack_from("<II", data, leaf)
            results.append((type_id, name_id, data_rva, data_size))

    return results


def extract_resources(exe_path: str) -> list[tuple[int, bytes]]:
    """Parse the PE file and return all CUSTOM (non-standard-type) resources
    as a list of (resource_id, raw_bytes) pairs."""
    with open(exe_path, "rb") as f:
        exe = f.read()

    # DOS header → PE offset
    if exe[:2] != b"MZ":
        raise ValueError("Not a PE file (missing MZ signature)")
    pe_offset = struct.unpack_from("<I", exe, 0x3C)[0]
    if exe[pe_offset : pe_offset + 4] != b"PE\0\0":
        raise ValueError("PE signature not found")

    # COFF file header
    coff = pe_offset + 4
    num_sections = struct.unpack_from("<H", exe, coff + 2)[0]
    opt_hdr_size = struct.unpack_from("<H", exe, coff + 16)[0]

    # Optional header
    opt = coff + 20
    pe_magic = struct.unpack_from("<H", exe, opt)[0]
    if pe_magic == 0x10B:  # PE32
        data_dir_offset = opt + 96
    elif pe_magic == 0x20B:  # PE32+
        data_dir_offset = opt + 112
    else:
        raise ValueError(f"Unknown PE magic 0x{pe_magic:04X}")

    # Resource directory entry (index 2)
    rsrc_rva, rsrc_size = struct.unpack_from("<II", exe, data_dir_offset + 2 * 8)
    if rsrc_rva == 0:
        raise ValueError("No resource section found")

    # Section headers
    section_table = opt + opt_hdr_size
    sections = []
    for i in range(num_sections):
        sh = section_table + i * 40
        virt_size = struct.unpack_from("<I", exe, sh + 16)[0]
        virt_addr = struct.unpack_from("<I", exe, sh + 12)[0]
        raw_size = struct.unpack_from("<I", exe, sh + 16)[0]
        raw_off = struct.unpack_from("<I", exe, sh + 20)[0]
        sections.append((virt_addr, virt_size, raw_off, raw_size))

    rsrc_file_off = rva_to_file_offset(rsrc_rva, sections)

    # Walk the resource tree.
    # Standard Windows resource types are IDs 1–24.  Firmware blobs are stored
    # under a CUSTOM type (numeric ID > 24).  We collect all leaves and filter
    # to those whose type_id is non-standard.
    all_leaves = parse_resource_directory(
        exe, rsrc_file_off, rsrc_rva, 0, 0, None, None
    )

    # Named type string (tid=None) → VB6 custom resource → firmware blob.
    # Numeric type IDs 1–24 are standard Windows resources; skip them.
    standard_types = set(range(1, 25))
    custom = [
        (nid, rva, sz)
        for tid, nid, rva, sz in all_leaves
        if (tid is None or tid not in standard_types) and nid is not None
    ]

    results = []
    for name_id, data_rva, data_size in custom:
        file_off = rva_to_file_offset(data_rva, sections)
        payload = exe[file_off : file_off + data_size]
        results.append((name_id, payload))

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    exe_path = sys.argv[1] if len(sys.argv) > 1 else "eb.exe"
    out_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(".")

    print(f"Parsing {exe_path} ...")
    resources = extract_resources(exe_path)

    if not resources:
        print("No CUSTOM resources found.")
        sys.exit(1)

    print(f"Found {len(resources)} firmware resource(s):\n")
    print(f"  {'ID':>6}  {'Device':20}  {'Size':>8}  Output file")
    print(f"  {'──':>6}  {'──────':20}  {'────':>8}  ───────────")

    for res_id, payload in sorted(resources):
        device = DEVICE_NAMES.get(res_id, f"unknown_0x{res_id:X}")
        out_name = f"firmware_id{res_id}_{device}.bin"
        out_path = out_dir / out_name
        out_path.write_bytes(payload)
        print(f"  {res_id:>6}  {device:20}  {len(payload):>8}  {out_name}")

    print(
        f"\nFlash base address: 0x{FLASH_BASE:08X} (written after the 36 KB bootloader)"
    )
    print("Use extract_firmware.py on an update pcap to verify a specific device.")


if __name__ == "__main__":
    main()
