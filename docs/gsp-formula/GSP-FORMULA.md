# How Smash Ultimate 13.0.5 computes GSP

2026-10-30. Static analysis of the 13.0.5 `main` (addresses are memory
addresses; file offset = address + 0x100), checked against Eden LAN matches.
Every number in the worked examples below is reproduced exactly by
[tools/gsp_update.py](tools/gsp_update.py) and
[tools/gsp_display.py](tools/gsp_display.py), which follow the decompiled
code in float32.

## Summary

GSP is two separate things:

1. **A per-fighter rating**, updated by the console after each online match
   with one step of **Glicko-2** (Glickman's algorithm, scale 173.7178), times
   a ruleset multiplier, and uploaded to the server. The server stores what it
   is sent (DataStore 0x73/82).
2. **The displayed GSP**, computed on the console from that rating with a
   normal-curve formula whose parameters (and a 50-point table for the tails)
   come from the server.

So a win or loss changes the rating by a Glicko-2 amount; the GSP change you
see is that rating change pushed through the curve.

## What is stored per fighter

The rating record (cache `*(wrapper+8)+0xc0 + slot*0x20`; DataStore 82/84
wire: version, slot, three u32, time u64, flag u32) has three numbers:

| Field | Meaning | Used for |
| --- | --- | --- |
| A (record +8) | rating | start point of every update |
| B (record +0xc) | rating deviation (Glicko RD) | update |
| C (record +0x10) | displayed rating | GSP display, Elite test (`17049d0`: C >= setting 1201) |

Every update writes C = A (`17a6734..6738`). They only differ if something else
writes C: an Eden seed that changed only C changed the display (1641 shown as
14,727,480) while the next update still started from A = 1464.

An unplayed fighter gets A = C = min(s1003, s1000 + trunc((M − s1000)/2)) and
B = s1001, where M is the highest A among the fighters the server returned
(`172d078..d1e8`; s = setting). With the captured settings, an unplayed
fighter is 1518/130 once another fighter has 1536 (e92 below). On a profile
with no records, unplayed fighters behaved as 1500/130 (e91 below); that
branch was not traced.

## Settings

Settings come from the server (Utility GetIntegerSettings, which overrides the
game's own parameter resource; getter `1739120`). The values below come from
the Nintendo capture that Eden replays (`share/eden/lan-online/init/0x6e_7.bin`,
431 keys). The live server can change them; 1201 in particular. The float
getter `17393f0` divides the integer by a per-key divisor from the game's
parameter file. The divisors (1000 for σ, 100 for K) are inferred: they are
the only ones that reproduce the Eden records.

| Key | Value | Use |
| --- | --- | --- |
| 1000 | 1500 | Glicko origin r0; default rating |
| 1001 | 130 | default RD |
| 1002 | 60 → σ = 0.06 | volatility fed to every update (6001 = 60 in mode 0xd) |
| 1003 | 1595 | cap of the default for unplayed fighters |
| 1010..1030 | 50..100 → K = 0.5..1.0 | ruleset multiplier (table below) |
| 1050..1073 | see code | free-for-all/team factors (not used in 1v1) |
| 1201 | 1642 | Elite threshold on C |
| 1300..1304 | 1, 1500, 110, 1995587, 400 | display curve: on, μ, σ, floor F, band w |

## The update after a match (1v1)

Code: `16ac1c0` → `16aa370` → `17a55a0` (setup, K, clamp) → `16febb0`
(Glicko-2 step); outcome callback `16ae080`. Inputs per player come from a
4-slot table at `0x5313f08` (stride 0xb0), filled at `169dc80..169dda4`. Your
own A, B and C come from your cache record for the chosen fighter; each
opponent's come from data received for that player. Old values are at
+0x60/+0x68/+0x70 and new ones at +0x64/+0x6c/+0x74.

Inputs: your A, B; the opponent's A, B; the result s; settings 1000, 1002 and
the ruleset key.

1. **Result.** s = 1 if your final placement number is lower than the
   opponent's, else 0 (`16ae080`). A tie scores 0 for both.
2. **Glicko-2, one game** (`16febb0`), with μ = (A − 1500)/173.7178,
   φ = B/173.7178, and the same for the opponent:
   - g = 1/√(1 + 3φⱼ²/π²), E = 1/(1 + exp(−g(μ − μⱼ)))
   - v = 1/(g²E(1 − E)), Δ = v·g(s − E)
   - volatility σ′ by the Illinois iteration on f(x) with τ = 0.5, ε = 1e-5,
     a = ln σ². Here the loop stops on |f(A) − f(B)| ≤ ε, not on |B − A|.
     σ′ only feeds the next line; it is never stored.
   - φ* = √(φ² + σ′²), φ′ = 1/√(1/φ*² + 1/v), μ′ = μ + φ′²·g(s − E)
   - r′ = 1500 + 173.7178·μ′, RD′ = 173.7178·φ′
3. **Ruleset multiplier and clamp** (`17a6534..6738`):
   - new A = max(1, A + trunc(K·(r′ − A)))
   - new B = trunc(RD′)
   - new C = new A

   Two further factors multiply the change (`17a66cc..66f8`). Both are 1.0 in
   1v1; only free-for-all (3 or more players) and team battles set them
   (`17a4380`, settings 1050..1073).

Ruleset → K (`17a6564..66c8`). The rule fields are the bytes at `530a975`,
`530a97a` and `530a97b` and the u16 at `530a978`. The labels are inferred
from those values:

| `530a975` | Selector | Key → K |
| --- | --- | --- |
| 0 (time) | `530a978` seconds 120 / 150 / 180 | 1010 / 1011 / 1012 → 0.9 / 1.0 / 1.0; any other time → 1.0 |
| 1 | `530a97a` = 1 / 2 / 3 | 1013 / 1014 / 1015 → 0.7 / 1.0 / 1.0 |
| 2 | `530a97a` = 1, `530a97b` = 4..8 | 1016..1020 → 0.5, 0.7, 0.9, 1.0, 1.0 |
| 2 | `530a97a` = 2, `530a97b` = 4..8 | 1021..1025 → 0.7, 0.9, 1.0, 1.0, 1.0 |
| 2 | `530a97a` = 3, `530a97b` = 4..8 | 1026..1030 → 0.9, 1.0, 1.0, 1.0, 1.0 |

### When it runs

- **Match start:** `16aa770` → `16aa370` with a result callback that returns
  0.0 (`16ab8d0`) and no cache write. Every player uploads the record they
  would have after a **loss**, so quitting counts as a loss.
- **Match end:** `16ac3d0` → `16aa370` with the real result and a cache write
  (`1704460`) before the TaskRating 82 upload. It starts again from the
  pre-match A and B, not from the provisional record.
- `17a4380` also computes every player's new A, B and C for the result
  screen. The screen's "after" GSP is computed locally, before the server
  replies.

## The displayed GSP

Code: `17d1480(owner, kind, rating)` with kind 21. The result screen calls it
at `1d5add0` (old C, +0x70) and `1d5ae10` (new C, +0x74). The character
select screen value for C = 1641 matches the same formula; its call site was
not traced. Inputs: C, settings 1300..1304, and the
server's kind-21 table, cached in the save (`system_data.bin`): count, total
T, then up to 50 (rank, rating) pairs.

- **For μ − w ≤ C ≤ μ + w (1100..1900 with the captured settings):**
  GSP = trunc(F + (T − F)·Φ((C − μ)/σ)), at least 1. This is computed as
  `0.5·erfcf((C − μ)/(σ·−√2))` with μ = 1500, σ = 110 and F = 1,995,587.
- **Outside that band:** GSP = curve(band edge) + table(C) − table(band edge),
  where table(r) = T − rank(r) + 1. rank(r) is linear between the two pairs
  around r: 1 above the top pair and T below the last.
- **With setting 1300 = 0, or for other kinds:** table(r) alone.

The table and T come from the server: the same values are in Nintendo's
captured DataStore 0x73/60 reply that Eden replays. The Eden profiles' saves
have T = 16,141,400 and 50 pairs from {1, 3242} to {16,141,400, 1}, after two
timestamps (2026-06-24 and 20 days later). The 60-reply decoder was not
traced.

## Worked examples (Eden, 2026-10-30)

Settings as captured; T = 16,141,400; 2-minute timed matches (K = 0.9).

| Case | Inputs (A/B vs opp A/B, s) | Computed | Observed |
| --- | --- | --- | --- |
| e91 match start, both | 1500/130 vs 1500/130, 0 | 1464/123 | stored 1464/123 |
| e91 Shulk wins | 1500/130 vs 1500/130, 1 | 1536/123 | stored 1536 |
| e91 Lucario loses | 1500/130 vs 1500/130, 0 | 1464/123 | stored 1464/123 |
| e92 Lucario match start | 1464/123 vs 1518/130 (Isabelle, unplayed), 0 | 1436/117 | stored 1436/117 |
| e92 Lucario wins | 1464/123 vs 1518/130, 1 | 1501/117 | stored 1501/117 |
| Display, C = 1500 | unplayed fighter | 9,068,494 | 9,068,494 |
| Display, C = 1536 | after e91 win | 10,882,966 | 10,882,966 |
| Display, C = 1464 | after e91 loss | 7,254,021 | 7,254,021 |
| Display, C = 1641 | e92 C-only seed | 14,727,480 | 14,727,480 |

With an opponent at 1500 instead of 1518, the e92 rows would be 1435 and
1500, so the opponent's input really is their own stored A. Elite at C ≥ 1642
is GSP 14,749,911 with this T: the game's own converter returned that in Eden
(2026-10-30, through the gsp-visualizer plugin), one more than
`tools/gsp_display.py` gives.

## Limits

- 1v1 only. The free-for-all averaging and team code paths are described from
  the code (opponent RD = √ΣRDⱼ², new ratings averaged over opponents, factors
  from settings 1050..1073) but not checked against matches. Mode 0xd (rating
  ÷ 1000, σ from 6001) was not identified.
- Settings, T and the table are a capture from one date. On Nintendo's live
  server they can differ and change over time, so the same rating can show a
  different GSP on a different day.
- Whether Nintendo's server accepts or recomputes the uploaded record cannot
  be seen from the client. Eden stores it unchanged.
- The per-key divisors for 1002 and 1010..1030 are inferred from the fit, not
  read from the game's parameter file. `expf`/`logf`/`erfcf` are modelled in
  double precision, then rounded.
