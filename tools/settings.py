"""Print a Utility GetIntegerSettings reply (u32 count, then u16 key + s32 value).
Usage: settings.py <0x6e_7.bin> [key ...]   (all keys when none are given)"""
import struct
import sys

data = open(sys.argv[1], 'rb').read()
count = struct.unpack_from('<I', data, 0)[0]
values = {}
for i in range(count):
    key, value = struct.unpack_from('<Hi', data, 4 + 6 * i)
    values[key] = value
assert 4 + 6 * count == len(data), (count, len(data))
wanted = [int(k) for k in sys.argv[2:]] or sorted(values)
for key in wanted:
    print(key, values.get(key, '(absent)'))
