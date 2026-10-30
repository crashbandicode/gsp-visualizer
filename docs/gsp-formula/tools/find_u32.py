"""Find little-endian u32 values in files under a directory (files <= 64 MiB).
Usage: find_u32.py <dir> <decimal_value>..."""
import os
import struct
import sys

root, values = sys.argv[1], [int(v) for v in sys.argv[2:]]
needles = {struct.pack('<I', v): v for v in values}
for dirpath, _, names in os.walk(root):
    for name in names:
        path = os.path.join(dirpath, name)
        try:
            if os.path.getsize(path) > 64 << 20:
                continue
            data = open(path, 'rb').read()
        except OSError:
            continue
        for needle, v in needles.items():
            pos = data.find(needle)
            while pos != -1:
                print(f'{path} +0x{pos:x} {v}')
                pos = data.find(needle, pos + 1)
