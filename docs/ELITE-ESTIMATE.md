# Elite Smash threshold and match estimates

2026-10-30. Static analysis of the 13.0.5 `main` (memory addresses; file
offset = address + 0x100) and the float32 model in
[gsp-formula/tools/gsp_update.py](gsp-formula/tools/gsp_update.py), which
reproduces every observed Eden rating (see
[gsp-formula/GSP-FORMULA.md](gsp-formula/GSP-FORMULA.md)).

## Where the threshold comes from

- The Elite Smash test is `17049d0`: setting 9010 must be 1, then the
  fighter is in Elite Smash when its displayed rating C (cache record
  +0x10) is at least setting 1201 (`1704a80..1704a9c`, `cmp w0, w19`).
- Settings come from the server's Utility GetIntegerSettings reply (a
  key → s32 map). `1684e48..1684ea8` copies each received value into the
  game's parameter override slot (+0x38, flag +0x3c), so the getter
  `1739120` returns the server's value and falls back to the game's own
  parameter resource only for keys the server did not send.
- The value is a rating, not a GSP. Nintendo's captured reply (the one Eden
  replays, `share/eden/lan-online/init/0x6e_7.bin`) has 1201 = 1642. The
  game's converter turns it into GSP 14,749,911 with the captured total of
  16,141,400 players (Eden, 2026-10-30; the Python model gives one less).
- The GSP shown for the threshold is the game's own converter
  `17d1480(owner, 21, 1201)`, the same call the result screen and character
  select use. The threshold's GSP therefore moves when the server's total or
  table changes, even when 1201 does not.

Not traced: the network code that fills the received map, and the parameter
resource's own default for 1201.

## The estimates

One match is one call of the game's update with the current settings: r0 =
setting 1000, σ = setting 1002 (÷1000), K from the current rules (setting
1010..1030, ÷100, as `17a6564..66c8` selects it; 1.0 when no key applies).

- **To reach Elite (below the threshold):** wins in a row against an
  opponent at your own rating, until A ≥ setting 1201. This is a realistic
  lower bound: a win against a higher-rated opponent gains more, but
  matchmaking pairs players of similar rating.
- **To lose Elite (at or above it):** losses in a row against an opponent
  at your own rating, until A < setting 1201. Losing to a lower-rated
  opponent costs more, so it can take fewer losses. The lowest opponent
  rating matchmaking allows was not traced.
- **Opponent deviation:** the settled deviation, the smallest B after 64
  alternating matches from the default record (setting 1000, setting
  1001). With the captured settings it is 60, which every played fighter
  reaches. A lower opponent deviation moves the rating more, which keeps
  the win count a lower bound.
- Matches that end before the result upload count as losses: every player
  uploads the loss record at match start (`16aa770`).
- The count is capped at 999 (shown as "999+").

## Examples (threshold 1642, opponent deviation 60)

| Fighter (A/B) | K 0.9 (2-minute time) | K 1.0 |
| --- | --- | --- |
| Unplayed, 1500/130 | 5 wins | 4 wins |
| Unplayed next to a 1536 fighter, 1518/130 | 4 wins | 4 wins |
| Settled at 1500/60 | 16 wins | 15 wins |
| 1600/60 | 5 wins | 5 wins |
| 1620/60 | 3 wins | 3 wins |
| 1635/60 | 1 win | 1 win |
| 1642/60 | 1 loss | 1 loss |
| 1700/60 | 7 losses | 6 losses |
| 1700/60, opponents 200 lower | 5 losses | 4 losses |
| 1800/60 | 18 losses | 16 losses |
| 2000/60 | 40 losses | 36 losses |

A settled fighter gains or loses about 9 to 10 rating points per match
against an equal opponent at K 0.9, so the counts grow with the distance to
the threshold. These are the plugin's numbers: `src/glicko.rs` is checked
against the Python model on 304 generated vectors.

## Limits

- Settings, the threshold and the GSP table are the server's and can change
  at any time; the plugin reads them live, these examples use the capture.
- The rule fields at `530a975..530a97b` (the ones the update reads after a
  match) hold the Preferred Rules while the character select is open:
  Time 2:00 read as 0/120 (K 0.9), Time 3:00 as 0/180 (K 1.0) in Eden. The
  rules of the match itself come from matchmaking and can differ.
- 1v1 only: free-for-all and team matches use other factors (settings
  1050..1073), so with Format set to Smash or Team Battle the counts are
  only indicative.
