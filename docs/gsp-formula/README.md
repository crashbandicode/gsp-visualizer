# GSP formula research

The investigation behind gsp-visualizer (formerly its own repository,
ssbu-gsp-formula; its history is merged here).

How Super Smash Bros. Ultimate 13.0.5 computes Global Smash Power (GSP) after
an online win or loss, read from the 13.0.5 executable and checked against
numbers seen in Eden LAN matches (2026-10-30).

**In short:** each fighter has a stored rating A, a deviation B and a display
value C. After a 1v1 match the console runs one Glicko-2 step against the
opponent's A and B, scales the change by a ruleset multiplier (0.9 for a
2-minute timed match), and stores the new A with C = A. The GSP on screen is
F + (T − F)·Φ((C − 1500)/110), where the floor F, the player total T and a
50-point table for ratings outside 1100..1900 come from the server. At match
start every player uploads the record they would have after a loss.

| Eden match | Computed | Observed |
| --- | --- | --- |
| 1500/130 beats 1500/130 | 1536, shown 10,882,966 | 1536, 10,882,966 |
| 1500/130 loses to 1500/130 | 1464, shown 7,254,021 | 1464, 7,254,021 |
| 1464/123 beats 1518/130 | 1501/117 | 1501/117 |

Full formula, inputs, code addresses, settings and limits:
[GSP-FORMULA.md](GSP-FORMULA.md).

## Tools

Run from this directory:

```text
python3 tools/gsp_update.py 1500 130 1500 130 win      # new A B C
python3 tools/gsp_display.py 16141400 1500 1536        # displayed GSP
```

`peek.py`, `scan_imm.py`, `bl_callers.py`, `settings.py` and `find_u32.py`
are the small readers used on the flat `main` image, the captured settings
reply and the save.
