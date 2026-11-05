#!/usr/bin/env python3
"""Prints DOA2U's unlock state from the running game's save in memory. Read-only.

Offsets and what is known of them are under "Unlocks" in docs/doa2u-notes.md."""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from doa2u_live import Guest

SAVE_POINTER = 0x87CFA8


def main():
    guest = Guest()
    if guest.off is None:
        sys.exit("The running game does not contain DOA2's executable: is DOA2U loaded?")
    save = guest.u32(SAVE_POINTER)
    if save == 0:
        sys.exit("The save is not in memory yet.")
    print(f"save at {save:#x}")
    for name, offset in [
        ("flag +0x2ED2", 0x2ED2),
        ("flag +0x2ED3", 0x2ED3),
        ("flag +0x2ED4", 0x2ED4),
        ("flag +0x2ED5 (ID 11 in the select list)", 0x2ED5),
        ("flag +0x2ED6 (IDs 21 and 23 in the select list)", 0x2ED6),
    ]:
        print(f"{name}: {guest.r(save + offset, 1)[0]}")
    print("costumes per slot, +0x2ED8:", list(guest.r(save + 0x2ED8, 15)))
    print("per slot counters, +0x2EEC:", list(guest.r(save + 0x2EEC, 30)))
    print("counters, +0x2E48:", list(struct.unpack("<69H", guest.r(save + 0x2E48, 138))))
    roster = guest.r(0x6E20A0, 16 * 0x1C)
    ids = [struct.unpack_from("<I", roster, i * 0x1C)[0] for i in range(16)]
    count = guest.r(0x6E2260, 1)[0]
    print(f"select list in memory, {count} shown of 16:", list(guest.r(0xDF5D18, count)), "IDs by entry:", ids)


if __name__ == "__main__":
    main()
