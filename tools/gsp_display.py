"""Displayed GSP for a stored rating C, as decompiled from 0x17d1480 with kind 21
(the online GSP shown on the character select and result screens). Float32 is
emulated by rounding after each operation; erfcf is rounded from double.

Usage: gsp_display.py <total> <rating>...          (curve only, no table)
       gsp_display.py --solve-total <gsp>@<rating>... (totals that fit all pairs)
total is the save's player total (+0x22ec of the kind-21 table). Settings:
1301 mu=1500, 1302 sigma=110, 1303 floor=1995587, 1304 band=400 (captured values).
Inside mu +- band the table is not used; outside it the table's shape is added
to the curve value at the band edge (not modelled here)."""
import math
import struct
import sys


def f(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


MU, SIGMA, FLOOR, BAND = 1500.0, 110.0, 1995587.0, 400.0
NEG_SQRT2 = f(-1.4142135381698608)  # 0x4473038


def curve(total, rating):
    z = f(f(f(rating) - f(MU)) / f(f(SIGMA) * NEG_SQRT2))
    cdf = f(f(math.erfc(z)) * 0.5)
    value = f(f(FLOOR) + f(f(f(total) - f(FLOOR)) * cdf))
    return max(1, int(value))          # fcvtzu, then at least 1


def display(total, rating):
    if not MU - BAND <= rating <= MU + BAND:
        raise ValueError('outside the curve band: the table is needed')
    return curve(total, rating)


if __name__ == '__main__':
    if sys.argv[1] == '--solve-total':
        pairs = [tuple(int(x) for x in p.split('@')) for p in sys.argv[2:]]
        g0, r0 = pairs[0]
        guess = int(2 * (g0 - FLOOR) + FLOOR) if r0 == MU else 16_000_000
        fits = [t for t in range(guess - 5000, guess + 5000)
                if all(display(t, r) == g for g, r in pairs)]
        print('totals fitting all pairs:', fits[:10], '...' if len(fits) > 10 else '')
    else:
        total = int(sys.argv[1])
        for r in sys.argv[2:]:
            print(r, display(total, int(r)))
