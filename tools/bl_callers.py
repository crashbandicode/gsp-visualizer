"""List BL/B instructions in the flat 13.0.5 main that target the given addresses.
Usage: bl_callers.py <image> <hex_target>..."""
import struct
import sys
from array import array

BASE = 0x100
image = open(sys.argv[1], 'rb').read()
targets = {int(a, 16) for a in sys.argv[2:]}
text_size = struct.unpack_from('<I', image, 0x18)[0]
words = array('I')
words.frombytes(image[BASE:BASE + text_size])
for i, w in enumerate(words):
    if w & 0x7c000000 == 0x14000000:            # B / BL imm26
        imm = w & 0x3ffffff
        if imm & (1 << 25):
            imm -= 1 << 26
        target = i * 4 + imm * 4
        if target in targets:
            print(f'0x{i * 4:x}: {"bl" if w & 0x80000000 else "b "} 0x{target:x}')
