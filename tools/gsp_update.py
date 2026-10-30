"""Smash Ultimate 13.0.5 post-match rating update for one 1v1 match, as decompiled
from 0x17a55a0 (setup, K, clamp) and 0x16febb0 (Glicko-2 step). Float32 is emulated
by rounding after each operation; libm calls are rounded from double precision.

Usage: gsp_update.py <A> <B> <opp_A> <opp_B> <win|loss> [K] [sigma] [r0]
  A, B         own stored rating and deviation (record +8 / +0xc; record +0x10 is C)
  opp_A, opp_B opponent's
  K            ruleset multiplier (setting 1010..1030 / divisor), default 0.9
  sigma        volatility (setting 1002 / divisor), default 0.06
  r0           rating origin (setting 1000), default 1500
Prints the new A, B and C (C is always set to the new A)."""
import math
import struct
import sys


def f(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


SCALE = f(173.71780395507812)  # 0x4471fe8
PI2 = f(9.86960506439209)      # 0x4472f44
TAU = f(0.5)                   # context +0x10
EPS = f(1e-5)                  # context +0xc (0x3727c5ac)


def g(phi):
    return f(1.0 / f(math.sqrt(f(f(f(f(phi * 3.0) * phi) / PI2) + 1.0))))


def glicko2(r, rd, sigma, r0, opp_r, opp_rd, s):
    """0x16febb0 with one opponent. Returns (r', rd')."""
    mu, phi = f(f(r - r0) / SCALE), f(rd / SCALE)
    mu_j, phi_j = f(f(opp_r - r0) / SCALE), f(opp_rd / SCALE)
    g_j = g(phi_j)
    e_j = f(1.0 / f(f(math.exp(f(-f(f(mu - mu_j) * g_j)))) + 1.0))
    v = f(1.0 / f(f(f(g_j * g_j) * e_j) * f(1.0 - e_j)))
    gsum = f(g_j * f(s - e_j))
    delta = f(gsum * v)
    # Volatility: Illinois iteration on f(x), as in Glickman's Glicko-2 step 5.
    d2, p2 = f(delta * delta), f(phi * phi)
    a = f(math.log(f(sigma * sigma)))

    def fx(x):
        ex = f(math.exp(x))
        num = f(ex * f(f(f(d2 - p2) - v) - ex))
        den = f(f(f(p2 + v) + ex) ** 2)
        return f(f(num / f(den + den)) - f(f(x - a) / f(TAU * TAU)))

    A = a
    if d2 > f(p2 + v):
        B = f(math.log(f(f(d2 - p2) - v)))
    else:
        B = f(a - TAU)
        while fx(B) < 0:                     # 0x16feec4..0x16fef00
            B = f(B - TAU)
    fa, fb = fx(A), fx(B)
    # 0x16fef84..0x16ff034: the game stops on |f(A) - f(B)| <= eps (Glickman's
    # paper uses |B - A|), and exits early when f(C) or f(B) is exactly 0.
    while abs(f(fa - fb)) > EPS:
        C = f(A + f(f(f(A - B) * fa) / f(fb - fa)))
        fc = fx(C)
        if fc == 0:
            A = C
            break
        if fb == 0:
            A = B
            break
        if f(fb * fc) < 0:
            A, fa = B, fb
        else:
            fa = f(fa * 0.5)
        B, fb = C, fc
    sigma_new = f(math.exp(f(A * 0.5)))
    phi_star = f(math.sqrt(f(f(sigma_new * sigma_new) + p2)))
    phi_new = f(1.0 / f(math.sqrt(f(f(1.0 / f(phi_star * phi_star)) + f(1.0 / v)))))
    mu_new = f(mu + f(f(phi_new * phi_new) * gsum))
    return f(r0 + f(mu_new * SCALE)), f(phi_new * SCALE)


def update(a, b, opp_a, opp_b, s, k=0.9, sigma=0.06, r0=1500.0):
    """0x17a55a0, 1v1 path (asymmetry factors 1.0, scale 1.0)."""
    r_new, rd_new = glicko2(f(a), f(b), f(sigma), f(r0), f(opp_a), f(opp_b), f(s))
    change = f(f(f(r_new - f(a)) * f(k)) * 1.0)
    new_a = max(1, a + int(change))          # fcvtzs truncates toward zero
    return new_a, int(rd_new), new_a


if __name__ == '__main__':
    args = sys.argv[1:]
    a, b, oa, ob = (int(x) for x in args[:4])
    s = 1.0 if args[4] == 'win' else 0.0
    extra = [float(x) for x in args[5:]]
    print(*update(a, b, oa, ob, s, *extra))
