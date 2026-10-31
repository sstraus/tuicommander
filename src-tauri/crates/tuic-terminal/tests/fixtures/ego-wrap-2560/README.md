# Real ego answer fixture for story 2560-b448

Source: installed ego 0.4.0 in a real TUIC PTY created without explicit dimensions. `stty size` reported 24 rows and 80 columns. A local deterministic streaming provider supplied the Italian text. Ego emitted the answer marker and answer text.

`answer.txt` is reconstructed from the answer rows in the recorded `ego80-before.bin` styled grid. The decoder omits zero-codepoint wide-cell spacers and trims final row padding. It does not insert row breaks or wrap flags. The regression test feeds this logical text through the real terminal parser and lets that parser create the wraps.

Original recordings and browser capture: `/Users/stefano.straus/Gits/.tmp/ego-wrap-bar/`. See `docs/internals/2026-10-31-ego-wrap-answer-gutter.md` for commands, layer diagnosis, and evidence limits.
