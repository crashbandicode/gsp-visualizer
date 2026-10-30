"""List .text instructions in the flat 13.0.5 main that use given immediates:
32/64-bit LDR/STR with an unsigned byte offset, ADD/SUB #imm, or MOVZ #imm16.
Usage: scan_imm.py <image> <hex_imm>..."""
import struct
import sys
from array import array

BASE = 0x100
image = open(sys.argv[1], 'rb').read()
wanted = {int(a, 16) for a in sys.argv[2:]}
text_size = struct.unpack_from('<I', image, 0x18)[0]
words = array('I')
words.frombytes(image[BASE:BASE + text_size])
for i, w in enumerate(words):
    hit = None
    if w & 0x3b000000 == 0x39000000:            # LDR/STR (unsigned immediate)
        size = w >> 30
        off = ((w >> 10) & 0xfff) << size
        if off in wanted:
            hit = f'{"ldr" if w & 0x400000 else "str"} size{1 << size} #{off:#x}'
    elif w & 0x7f800000 == 0x11000000 or w & 0x7f800000 == 0x51000000:  # ADD/SUB imm
        imm = (w >> 10) & 0xfff
        imm <<= 12 if (w >> 22) & 1 else 0
        if imm in wanted:
            hit = f'{"sub" if w & 0x40000000 else "add"} #{imm:#x}'
    elif w & 0x7f800000 == 0x52800000:          # MOVZ
        imm = ((w >> 5) & 0xffff) << (16 * ((w >> 21) & 3))
        if imm in wanted:
            hit = f'movz #{imm:#x}'
    if hit:
        print(f'0x{i * 4:x}: {hit}')
