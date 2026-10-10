# Spoken replies visual evidence (1659-f3cc)

Captured and inspected on macOS, 2026-10-10, using headless Chrome 149 CDP
against this worktree's Vite server at a 390 × 844 phone viewport.

| Screenshot | Checkbox | Persisted preference | Hands-free armed |
| --- | --- | --- | --- |
| phone-spoken-replies-on.png | on | true | true |
| phone-spoken-replies-off.png | off | false | true |
| phone-spoken-replies-restored.png | on | true | true |
| phone-settings-on.png | on | true | false |
| phone-settings-off.png | off | false | false |

The screenshots render the actual SessionDetailScreen, SettingsScreen, global
styles and dictation store. Trusted CDP touch events toggle the checkbox.
The conversation label spans 390 × 44px at y=56, entirely inside the viewport.
The Settings label spans 358 × 38px at x=16, y=232; it is visible and unclipped.
Its 38px height is below the 44px touch target guideline and is recorded for the
coordinator; this evidence-only lane does not modify production styles.

visual-measurements.json retains each checkbox's computed size, checked and
disabled state, label bounds, viewport, store state and owned API request paths.
The off/on conversation captures demonstrate that changing the preference
leaves hands-free armed. Leaving the conversation for Settings disarms it
through the existing screen cleanup.

## Reproduce

From the worktree root:

    node docs/evidence/voice-mute-1659/capture.mjs

The script starts a worktree Vite with the Solid plugin and an isolated headless
Chrome profile under .runtime/, captures five PNGs, and stops both owned
processes. The profile and logs are ignored. TUIC_EVIDENCE_CHROME can select
another existing Chrome binary.

preview.tsx supplies only owned HTTP fixtures, including an in-memory
dictation configuration. The rendered UI and store are real; this proves the
checkbox interaction and save payload, not physical audio or Rust persistence.
Those server boundaries are covered by the story's targeted Rust tests.
No desktop TUIC, Boss session, credentials or external speech service is used.
Headless CDP is the coordinator-authorized replacement for the hanging stealth
wrapper capture when the Mac screen is locked.
