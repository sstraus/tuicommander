# Mobile question presentation regression

Run `scripts/with-test-tmp.sh bash scripts/mobile-question-visual/run.sh` through
the build-slot scheduler. Requires the bundled Chrome for Testing used by the
Settings visual harness. All browser state and PNGs stay under the test scratch root (`$TUIC_TEST_TMP_ROOT`, else `<temp dir>/tuic-tests`) in `mobile-question-visual/`, never inside the checkout.

The real SessionDetailScreen and mobile CSS render narrow ego question states
recorded by the Rust replay of the installed ego captures for story 1973.
`recorded-states.json` contains the 44-column entries from
`~/Gits/.tmp/ui-1973/backend-snapshots.json`; capture provenance is in the
committed `ego-*-44-1973` fixture Markdown files. Transport, terminal output and
voice availability are isolated presentation seams, not evidence of a live
backend, microphone or permission state.

`mobile_question_cards_keep_theme_contrast_and_clear_keybar` catches an opaque
but hard-coded dark card in Paper and question cards covering the key bar.
It checks both themes, three question forms, and 390/834 widths. The PNGs are
visual evidence; geometry and computed colors are the regression oracle.

Set `VISUAL_ONLY=1` for question screenshots without executing regression
assertions, or `VISUAL_VOICE=1` for phone-menu and tablet-header handset captures.
