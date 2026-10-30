"""Print the 64 bytes at each address of the flat 13.0.5 main as a Rust array,
for the plugin's guard checks (file offset = address + 0x100).
Usage: guard.py <image> <hex_addr>..."""
import sys

BASE = 0x100
image = open(sys.argv[1], 'rb').read()
for arg in sys.argv[2:]:
    addr = int(arg, 16)
    data = image[BASE + addr:BASE + addr + 64]
    rows = [', '.join(f'0x{b:02x}' for b in data[i:i + 16]) for i in range(0, 64, 16)]
    print(f'// 0x{addr:x}')
    print('[\n    ' + ',\n    '.join(rows) + ',\n]')
