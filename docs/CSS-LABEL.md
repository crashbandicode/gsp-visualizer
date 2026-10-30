# The character select label

2026-10-30. Static analysis of the 13.0.5 `main` (memory addresses; file
offset = address + 0x100), checked in Eden the same day (below).

## How the game shows a fighter's GSP there

- The online character select's score panel (panel mode `+0x16c` = 20)
  keeps a map at `+0x290` from fighter Hash40 to an info record. Building it
  (`1a29444..1a29538`), the game reads each fighter's GSP with `17994c0`
  (kind 21: the cached record's C through `17d1480`).
- Hovering a fighter (`1a19bf8..1a19c44`) looks the fighter up in that map
  and calls `1a1c070(part "set_parts_score_0", info)`. The map node holds the
  Hash40 key at node +0x20 and the info at node +0x28.
- `1a1c070` gets the part's `set_txt_match` pane. If the info holds a GSP
  (info +0 set and mode +0xc 1 or 2) it calls
  `377a280(pane, "mel_chara_select_world_pw_n", 1, info +0x10)`, otherwise
  `377a120(pane, "mel_chara_select_world_pw_none")`.
- `377a280` → `37a2760` looks the message up, formats the number with the
  language's `sys_digit_separator` and passes the text to `37a2270(pane,
  text, units, flags)`. That function grows the pane's string buffer to fit
  (TextBox vtable +0xe8) before setting the text (+0x110), so a longer text is
  not cut by the buffer.

## What the plugin does

- **Arm** (inline hook at `1a1c0ec`, the GSP path of `1a1c070`): only when
  the saved return address is `1a19c48`, the hover call. Reads the fighter
  key at info − 8, the fighter's A, B and C from the rating cache, settings
  9010, 1201, 1000, 1001, 1002 and the rules' K through the game's getters
  `1739120`/`17393f0`, and the threshold's GSP through `17d1480`. Stores the
  label and the pane (`*(*(sp + 8) + 0x10)`, the pane `377a280` will set).
- **Append** (function hook on `37a2270`): when the pane is the armed one,
  passes the game's text plus a line break and `Elite 14,749,911 · 5 wins`
  (or `· 7 losses` at or above the threshold), with the same thousands
  separator as the game's number. Any other text passes through unchanged.
- **Disarm** (inline hook at `1a1c118`, where both paths meet).

Every site and called function is checked against 64 bytes of the 13.0.5
image before anything is installed; any mismatch, or another game version,
installs nothing. The plugin writes no game state and sends nothing.

## Rating cache and other addresses

| What | Where |
| --- | --- |
| Rating state | `*(*(base + 0x52c4c58) + 8)` |
| Cached records (99, stride 0x20) | `*(state + 0xa8) + 0xc0 + slot·0x20`: header u64, A, B, C |
| Settings manager | `*(state + 0xb0)`, getters `1739120` (int), `17393f0` (float) |
| Fighter Hash40 → slot | `3264270(*(*(*(base + 0x532f730) + 8) + 0x168), hash)` |
| GSP converter owner | `*(*(*(*(base + 0x5314510)) + 0x68))`; table count at `*(owner + 8) + 0x22e8` |
| Rules (as the update reads them) | `530a975` rule, `530a978` seconds, `530a97a`, `530a97b` |

## Diagnostic build and tools

`cargo skyline build --release --features diag` (Eden only) logs each
label's inputs to `sd:/ultimate/gsp-visualizer/events.log` and reads
`sd:/ultimate/gsp-visualizer/diag.txt` every 2 s: `joiner=newline|space`,
`dot=middle|dash`, `threshold=<rating>|native`.

`tools/`: `gen_vectors.py` (the Rust test vectors, from the float32 model
in `docs/gsp-formula/tools/gsp_update.py`) and `guard.py` (hook-site guard
bytes). The models and the readers used on the flat `main` image are in
[gsp-formula/tools/](gsp-formula/tools/).

## Eden check (2026-10-30)

One Eden instance on LAN Online with the Eden agent's rating runtime, a
fresh profile (every fighter unplayed), English, Quickplay → Solo.

- The panel is the character select's right-hand box: "GSP" (part 0, the
  hovered fighter), "No.1 GSP" (part 1) and "Roster GSP" (part 2), as the
  code above names them. Only part 0 gets the label.
- Moving the cursor down a column armed the hook once per fighter, with
  slots 82, 69, 56, 43, 30, 17, 4, the record 1500/130/1500 and the shown
  GSP 9,068,494 each time; no other caller reached the GSP path.
- The text became `9,068,494⏎Elite 14,749,911 · 5 wins` (35 units). The
  line break fits inside the box and the font has `·`; both stay the
  defaults.
- With `threshold=1400` in `diag.txt` the label became
  `Elite 4,565,189 · 3 losses`.
- Preferred Rules Time 3:00 changed the rule fields to 0/180, K to 1.0 and
  the label to `4 wins`.
- The release build (no diagnostics) showed the same label; install line
  `result=installed hooks=arm,disarm,set_text heap_allocations=0`.
