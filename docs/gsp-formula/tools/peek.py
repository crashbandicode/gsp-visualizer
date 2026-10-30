"""Read values from the flat 13.0.5 main image (file offset = address + 0x100).
Usage:
  peek.py <image> f32 <addr>...        float at each address
  peek.py <image> u32 <addr>...        u32 at each address
  peek.py <image> plt <addr>...        import name behind each PLT stub
  peek.py <image> jt <table> <n>       targets of an i32 relative jump table
  peek.py <image> slots <start> <n>    n 8-byte slots with RELATIVE reloc targets"""
import struct
import sys

BASE = 0x100
image = open(sys.argv[1], 'rb').read()
mode, args = sys.argv[2], [int(a, 16) for a in sys.argv[3:]]


def u32(addr):
    return struct.unpack_from('<I', image, BASE + addr)[0]


def dynamic_tags():
    mod0 = u32(4)
    pos = mod0 + struct.unpack_from('<i', image, BASE + mod0 + 4)[0]
    tags = {}
    while True:
        tag, value = struct.unpack_from('<qQ', image, BASE + pos)
        if tag == 0:
            return tags
        tags[tag] = value
        pos += 16


if mode == 'f32':
    for a in args:
        print(f'0x{a:x}: {struct.unpack_from("<f", image, BASE + a)[0]!r} (0x{u32(a):08x})')
elif mode == 'u32':
    for a in args:
        print(f'0x{a:x}: {u32(a)} (0x{u32(a):08x})')
elif mode == 'plt':
    tags = dynamic_tags()
    symtab, strtab = tags[6], tags[5]
    slots = {}
    for off in range(tags[23], tags[23] + tags[2], 24):
        where, info, _ = struct.unpack_from('<QQq', image, BASE + off)
        name_off = u32(symtab + (info >> 32) * 24)
        end = image.index(b'\0', BASE + strtab + name_off)
        slots[where] = image[BASE + strtab + name_off:end].decode()
    for a in args:
        adrp, ldr = u32(a), u32(a + 4)
        imm = ((adrp >> 29) & 3) | (((adrp >> 5) & 0x7ffff) << 2)
        if imm & (1 << 20):
            imm -= 1 << 21
        got = (a & ~0xfff) + (imm << 12) + (((ldr >> 10) & 0xfff) << 3)
        print(f'0x{a:x}: got 0x{got:x} {slots.get(got, "?")}')
elif mode == 'jt':
    table, n = args
    for k in range(n):
        print(f'0x{table:x}[{k}] -> 0x{table + struct.unpack_from("<i", image, BASE + table + 4 * k)[0]:x}')
elif mode == 'slots':
    start, n = args
    tags = dynamic_tags()
    relocs = {}
    for off in range(tags[7], tags[7] + tags[8], 24):
        where, info, addend = struct.unpack_from('<QQq', image, BASE + off)
        if start <= where < start + 8 * n and info & 0xffffffff == 1027:
            relocs[where] = addend
    for slot in range(start, start + 8 * n, 8):
        if slot in relocs:
            print(f'0x{slot:x}: -> 0x{relocs[slot]:x}')
        else:
            print(f'0x{slot:x}: 0x{struct.unpack_from("<Q", image, BASE + slot)[0]:x}')
