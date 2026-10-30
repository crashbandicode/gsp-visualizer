# ssbu-gsp-formula

> **Disclaimer:** this project was developed with the assistance of AI
> (Claude, Codex and other coding agents) under the operator's direction and
> review. Nothing is claimed beyond the dated tests cited below.

How Super Smash Bros. Ultimate 13.0.5 computes Global Smash Power (GSP) after
an online win or loss: which numbers go in (the fighter's stored rating, the
opponent's, the result, the game's parameters, what the server sends), the
formula the game applies, and how the GSP on screen relates to the stored
rating. Read from the 13.0.5 executable and checked against numbers seen in
Eden LAN matches.

## Status

2026-10-30: started, no findings yet. Results will go in
[docs/GSP-FORMULA.md](docs/GSP-FORMULA.md), with code addresses and worked
examples that reproduce the observed numbers.
