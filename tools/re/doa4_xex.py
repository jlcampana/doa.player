#!/usr/bin/env python3
"""Writes DOA4's game image from the install's default.xex: AES with the retail key, then XEX2's
basic compression (runs of data and of zeros). Only reads the install.

  doa4_xex.py [XEX] [OUT]    defaults: $DOA4_GAME/default.xex (or the install's), target/doa4/image_82000000.bin

The image loads at 0x82000000 (the XEX's optional header 0x10201), so a guest address A is at
A - 0x82000000 in the file. xbox360_ppc.py disassembles it with IMAGE=<file>.
"""
import os, struct, sys
from Crypto.Cipher import AES

RETAIL_KEY = bytes.fromhex('20B185A59D28FDC340583FBB0896BF91')
GAME = os.environ.get('DOA4_GAME', '/home/pandi/Ports/Apps/DeadOrAlive4Online-DeadOrAlive4Online/game')
src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(GAME, 'default.xex')
out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(__file__), '../../target/doa4/image_82000000.bin')

d = open(src, 'rb').read()
magic, flags, pe_offset, _, security, count = struct.unpack_from('>4sIIIII', d, 0)
assert magic == b'XEX2', magic
options = dict(struct.unpack_from('>II', d, 24 + i * 8) for i in range(count))
base = options.get(0x10201)
info = options[0x3FF]
size, encryption, compression = struct.unpack_from('>IHH', d, info)
assert encryption == 1 and compression == 1, (encryption, compression)
# The file key, encrypted with the retail key, is at +0x150 of the security info.
key = AES.new(RETAIL_KEY, AES.MODE_CBC, bytes(16)).decrypt(d[security + 0x150:security + 0x160])
payload = AES.new(key, AES.MODE_CBC, bytes(16)).decrypt(d[pe_offset:pe_offset + (len(d) - pe_offset) // 16 * 16])
image = bytearray()
at = 0
for i in range((size - 8) // 8):
    data, zeros = struct.unpack_from('>II', d, info + 8 + i * 8)
    image += payload[at:at + data]
    image += bytes(zeros)
    at += data
assert image[:2] == b'MZ', image[:4]
os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
open(out, 'wb').write(image)
print(f'{out}: {len(image)} bytes, loads at {base:#x}')
