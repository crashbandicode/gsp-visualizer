#!/usr/bin/env python3
"""Generate deterministic Rust regression vectors from the float32 model."""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from tools.gsp_update import update  # noqa: E402


BASE_CASES = 38
K_VALUES = (0.5, 0.7, 0.9, 1.0)
OFFSETS = (-600, 600, -450, 450, -300, 300, -150, 150, -75, 75, -30, 30, -10, 10, 0)
OUTPUT = ROOT / "src" / "glicko_vectors.txt"


def vectors():
    for i in range(BASE_CASES):
        a = 1 + i * 2999 // (BASE_CASES - 1)
        b = 30 if i == 0 else 350 if i == BASE_CASES - 1 else 30 + (i * 71) % 321
        offset = OFFSETS[i % len(OFFSETS)]
        opp_a = min(3000, max(1, a + offset))
        opp_b = (
            30
            if i == 0
            else 350
            if i == BASE_CASES - 1
            else 30 + (i * 137 + 17) % 321
        )
        for k in K_VALUES:
            for win in (False, True):
                new_a, new_b, _ = update(a, b, opp_a, opp_b, 1.0 if win else 0.0, k, 0.06, 1500.0)
                yield f"{a} {b} {opp_a} {opp_b} {'W' if win else 'L'} {k:.1f} {new_a} {new_b}"


def main():
    lines = list(vectors())
    OUTPUT.write_text("\n".join(lines) + "\n", encoding="ascii")
    print(f"wrote {len(lines)} vectors to {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
