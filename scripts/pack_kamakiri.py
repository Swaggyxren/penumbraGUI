#!/usr/bin/env python3
#
# SPDX-FileCopyrightText: 2025 Shomy
# SPDX-License-Identifier: AGPL-3.0-or-later
#
import csv
import struct
from typing import Final

MAGIC: Final[bytes] = b"PENUMBRALC" + bytes([0] * 2)


def parse_csv(input_file: str) -> dict:
    data = {}
    with open(input_file, "r") as csvfile:
        reader = csv.DictReader(csvfile)
        for row in reader:
            hwcode = row["hwcode"]
            name = row["name"]
            wdt = row["wdt"]
            ptr_usbdl = row["ptr_usbdl"]
            ptr_da = row["ptr_da"]
            cert_addr = row["cert_addr"]
            data[hwcode] = {
                "name": name,
                "wdt": wdt,
                "ptr_usbdl": ptr_usbdl,
                "ptr_da": ptr_da,
                "cert_addr": cert_addr,
            }
    return data


def pack_kamakiri(data: dict, payload_file: str, output_file: str) -> None:
    header_format = "<12s3I"  # Magic (12 bytes) + Number of entries (4 bytes) + Payload offset (4 bytes) + Payload length (4 bytes)

    entries_format = "<5I"

    entries = []
    for hw, info in data.items():
        hwcode = int(hw, 16)
        wdt = int(info["wdt"], 16)
        ptr_usbdl = int(info["ptr_usbdl"], 16)
        ptr_da = int(info["ptr_da"], 16)
        cert_addr = int(info["cert_addr"], 16)

        entries.append(
            struct.pack(entries_format, hwcode, wdt, ptr_usbdl, ptr_da, cert_addr)
        )

    with open(payload_file, "rb") as f:
        payload = f.read()

    header_len = struct.calcsize(header_format) + len(entries) * struct.calcsize(
        entries_format
    )
    payload_offset = header_len
    payload_length = len(payload)

    with open(output_file, "wb") as f:
        f.write(
            struct.pack(
                header_format,
                MAGIC,
                len(entries),
                payload_offset,
                payload_length,
            )
        )

        f.writelines(entries)
        f.write(payload)


if __name__ == "__main__":
    import sys

    if len(sys.argv) != 4:
        print(
            f"Usage: {sys.argv[0]} <input_csv_file> <payload_file> <output_kamakiri_file>"
        )
        sys.exit(1)

    input_csv_file = sys.argv[1]
    payload_file = sys.argv[2]
    output_kamakiri_file = sys.argv[3]

    data = parse_csv(input_csv_file)
    pack_kamakiri(data, payload_file, output_kamakiri_file)
    print(f"Packed Penumbra Kamakiri file '{output_kamakiri_file}' has been created.")
