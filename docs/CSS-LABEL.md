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

One inline hook (since 0.2.0), at `1a1c118` in `1a1c070`, after the game wrote
the GSP text, where both paths meet:

- Acts only on the GSP path (info `+0` and mode `+0xc`, as the setter tests
  them) and only when the saved return address is `1a19c48`, the hover call.
- Reads the game's text from the pane (`*(*(sp + 8) + 0x10)`; TextBox
  string `+0xd8`, length `+0x112`), the fighter key at info − 8, the
  fighter's A, B and C from the rating cache, settings 9010, 1201, 1000,
  1001, 1002 and the rules' K through the game's getters
  `1739120`/`17393f0`, and the threshold's GSP through `17d1480`.
- Calls the game's setter `37a2270` with the game's text, a line break and
  `Elite 14,749,911 · 5 wins` (or `· 7 losses` at or above the threshold),
  with the same thousands separator as the game's number.

0.1.0 hooked the setter `37a2270` itself, which every UI text goes through.
0.2.0 only calls it, so a plugin that hooks the setter runs inside that call
and both keep working.

**Checks before anything is installed** (0.3.0, `src/chain.rs`, the rules
elite-rematch uses): the 64 bytes at each site must be the 13.0.5 ones,
apart from other plugins' Skyline hooks (a one-word `b`, or a far jump).

- The hooked site `1a1c118`: another plugin's far jump there gets chained
  (this hook goes on a small stub that then jumps on to that plugin's
  handler), and another plugin's one-word branch is relocated into this
  hook (kept only if this hook is a one-word branch too; otherwise the bytes
  are put back). The install line says `apply=<stock|chain|branch>/<near|far>`.
  Anything else there: `apply=foreign/skipped`, nothing installed.
- The hover call `1a19c44` must keep its own `bl` (a hook on it would move
  the return address the hook checks into that plugin's trampoline); hooks
  after it are fine.
- The functions it only calls may carry other plugins' hooks anywhere; calls
  go through them. The install line names those sites (`other_hook=<name>`).
- When a site the label relies on fails, the label stays off
  (`label=off site=<name>`), but the owner still hooks `1a1c118` for the
  plugins sharing it (`result=hook_only`).

Another game version installs nothing. The plugin writes no game state and
sends nothing.

**Sharing the hook with other plugins (inline-hook sharing v1,
`src/share.rs`).** Two inline hooks on one instruction collide, so a plugin
that wants `1a1c118` exports `ssbu_inline_site_1a1c118_v1()`, returning a
table with a `register(callback)` function, and looks that name up with
`nn::ro::LookupSymbol` before hooking. Every plugin gets the same answer in
any load order. The plugin whose table comes back owns the site: it
installs the one hook and calls every registered callback from it, after its
own. The others register their callback there and install nothing. The
install line says which: `role=owner … guests=<n>` or
`role=guest owner=<address>`. A plugin that registers before the owner's
`main` runs is still accepted: `register` then checks the version and reads
the site the way the owner will; after the owner's `main` it answers from
whether the owner hooked (a guest it turns down logs
`result=owner_refused`). ssbu-quiet-elite-switch uses the same files
(`share.rs`, `chain.rs`) for its own site.

**Plugin API v1**, for other plugins through `nn::ro::LookupSymbol` (any
load order, since Skyline loads every plugin before any `main` runs):

- `gsp_visualizer_hooked_sites_v1(out: *mut usize, capacity: usize) -> usize`:
  the main-image offsets this plugin has hooked (`0x1a1c118`; none when it
  did not install), so a plugin can leave them alone.
- `gsp_visualizer_label_v1(fighter: u64, out: *mut u16, capacity: usize) -> usize`:
  the label for a fighter's Hash40 (`Elite <GSP> · <n> wins|losses`,
  UTF-16, NUL-terminated; 0 when unavailable), to show it elsewhere without
  hooking. Call it on the game's UI thread.

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
- A real Elite fighter: on a same-PC pair, a Quickplay match (Sephiroth
  against Pyra/Mythra, canceled 24 s in) left Sephiroth's record (slot 82,
  1500/130) in Eden's rating server; relaunched with the seed `82:1700`,
  the game itself showed Quickplay (Elite Smash) with only Sephiroth lit,
  and the label read `15,653,112⏎Elite 14,749,911 · 2 losses`. Unplayed
  fighters then default to 1600 (13,398,614, `2 wins`), as the game's
  default for unplayed fighters (halfway to the best fighter) predicts.
- 0.2.0 with the elite-rematch plugin and `test-plugins/share-probe` (which
  wants the same instruction) loaded together: gsp-visualizer logged
  `result=installed role=owner hooks=apply`, the probe
  `result=registered role=guest … ok=true`; while hovering, the probe's
  callback ran 8 times from gsp-visualizer's one hook and the label read
  `Elite 14,749,911 · 2 losses` (Sephiroth) and `· 2 wins`. A first run
  showed the probe refusing (`guard_mismatch`) because it guarded the site
  before the election; both plugins now elect first.
- 0.3.0 (diag) with `foreign=inline` (before its own hook the plugin hooks
  `1a1c118` and `37a2288`, inside the text setter, as a plugin that does not
  share would), ssbu-quiet-elite-switch 0.2.0 (diag, the same),
  elite-rematch 0.3.1 and the share probe (run e15): install line
  `foreign=inline role=owner apply=branch/near label=on
  other_hook=set_text result=installed heap_allocations=0`. Hovering
  Sephiroth showed `15,653,112⏎Elite 14,749,911 · 2 losses`; the stand-in at
  `1a1c118` ran 10 times and the one in the setter 2,218 times, and the
  probe's callback still ran from this plugin's hook.
- The same with `foreign=far` (a far jump on `1a1c118` to a stand-in handler,
  run e16): `apply=chain/far … result=installed`; the label showed, the
  stand-ins ran (far 10, setter 2,206) and the probe's callback ran.
