#!/usr/bin/env python3
"""Unlocks DOA3 costumes and clear rewards in DOA3SAVE.DAT.

Usage: doa3_unlock_save.py SAVE [--apply]
Without --apply it only reports the changes. With --apply it first writes a
timestamped backup next to the save. Close the game first, because it rewrites
the save while running. Offsets are documented in docs/doa3-internals.md.
"""
import shutil
import subprocess
import sys
import time
from functools import reduce

SAVE_SIZE = 0x3C10
CHECKSUM = 0x3C0C
COSTUMES = 0x2A8E   # one byte per slot; the game clamps it to its own maximum
SLOTS = 0x13
CLEARED = 0x3AC1    # one byte per slot: cleared with this character
EXTRA_REWARD = 0x2AB5
ALL_CLEARED = 0x3BEA

# From default.xbe: slot of each character ID (0x309CB8), costume maximum (0x36806C).
SLOT_OF_ID = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 14, 10, 11, 15, 0, 0, 12, 16, 17, 18, 0, 0, 0, 13]
MAX_OF_ID = [3, 2, 2, 3, 2, 3, 2, 2, 2, 2, 1, 2, 2, 2, 2, 1, 1, 1, 1, 2, 2, 1, 1, 1, 2]


def unlocked(save):
    data = bytearray(save)
    for character, slot in enumerate(SLOT_OF_ID):
        data[COSTUMES + slot] = max(data[COSTUMES + slot], MAX_OF_ID[character])
    for offset in range(SLOTS):
        data[CLEARED + offset] = 1
    data[EXTRA_REWARD] = 1
    data[ALL_CLEARED] = 1
    data[CHECKSUM] = reduce(lambda a, b: a ^ b, data[:CHECKSUM], 0)
    return bytes(data)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    path = sys.argv[1]
    apply = "--apply" in sys.argv[2:]
    with open(path, "rb") as f:
        save = f.read()
    if len(save) != SAVE_SIZE:
        sys.exit(f"Unexpected size {len(save)}, expected {SAVE_SIZE}")
    if reduce(lambda a, b: a ^ b, save[:CHECKSUM], 0) != save[CHECKSUM]:
        sys.exit("Checksum does not match: not touching this file")

    new = unlocked(save)
    changes = [(i, a, b) for i, (a, b) in enumerate(zip(save, new)) if a != b]
    for offset, old, value in changes:
        print(f"{offset:#06x}: {old:#04x} -> {value:#04x}")
    print(f"{len(changes)} bytes differ")
    if not apply:
        print("Dry run. Use --apply to write.")
        return
    if subprocess.run(["pgrep", "-f", "DOA3.exe"], capture_output=True).returncode == 0:
        sys.exit("DOA3 is running: close it first, or it will overwrite the save")
    backup = f"{path}.{time.strftime('%Y%m%d-%H%M%S')}.bak"
    shutil.copy2(path, backup)
    with open(path, "wb") as f:
        f.write(new)
    print(f"Written. Backup: {backup}")


if __name__ == "__main__":
    main()
