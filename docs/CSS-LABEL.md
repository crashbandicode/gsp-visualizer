# The character select label

2026-10-30. Static analysis of the 13.0.5 `main` (memory addresses; file
offset = address + 0x100). Eden check pending.

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
  passes the game's text plus a line break and `Elite 14,749,910 · 3 wins`
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

## To check in Eden

- The label appears under the hovered fighter's GSP and fits the panel
  (line break or two spaces; `·` or `-` if the font lacks `·`).
- The rule fields hold the preferred Quickplay rules on the character
  select (the diagnostic build logs them with each label).
- The numbers in the diagnostic log match the record and the shown GSP.
