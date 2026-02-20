---
id: 311-4001
title: Remove ~107 lines of dead CSS from styles.css
status: complete
priority: P3
created: "2026-02-20T19:25:49.683Z"
updated: "2026-02-20T20:21:21.207Z"
dependencies: []
---

# Remove ~107 lines of dead CSS from styles.css

## Problem Statement

styles.css contains ~107 lines of dead rules: #ide-btn and #ide-btn:hover (~18 lines, ID never in DOM), #agent-stats rules (~17 lines, ID never in DOM), .section-title (~10 lines, class never applied), .dropdown.hidden (~3 lines, SolidJS uses Show not class toggle), dead .hidden panel rules for markdown/notes/diff/github-status (~12 lines), dead color-picker classes: .color-picker-row/.color-picker-preview/.color-picker-value/.color-picker-reset (~35 lines), .dictation-model-action (~5 lines, typo - should be plural), .dictation-model-ready (~3 lines, never applied), redundant #toolbar * user-select (~4 lines, already covered by * rule).

## Acceptance Criteria

- [ ] Verify before: grep each class/ID in all TSX files confirming zero usage
- [ ] Remove #ide-btn and #ide-btn:hover (lines 1256-1274)
- [ ] Remove #agent-stats rules (lines 1768-1784)
- [ ] Remove .section-title (lines 302-311)
- [ ] Remove .dropdown.hidden (lines 1290-1292)
- [ ] Remove dead .hidden panel rules for #markdown-panel, #notes-panel, #diff-panel, #github-status (keep #prompt-overlay.hidden)
- [ ] Remove .color-picker-row, .color-picker-preview, .color-picker-value, .color-picker-reset (keep .color-picker-swatch which IS used)
- [ ] Remove .dictation-model-action and .dictation-model-ready
- [ ] Remove redundant #toolbar * user-select rules
- [ ] Take screenshot to verify no visual regression
- [ ] Run make check and tests pass

## Files

- src/styles.css

## Work Log

### 2026-02-20T20:21:18.767Z - Removed 103 lines of dead CSS: #ide-btn, #agent-stats, .section-title, .dropdown.hidden, color-picker classes, dictation classes, #toolbar user-select

