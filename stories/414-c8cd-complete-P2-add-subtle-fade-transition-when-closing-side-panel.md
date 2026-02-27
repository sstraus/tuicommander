---
id: 414-c8cd
title: Add subtle fade transition when closing side panels (Diff, Markdown, FileBrowser, Notes)
status: complete
priority: P2
created: "2026-02-26T21:44:20.756Z"
updated: "2026-02-27T11:28:35.206Z"
dependencies: []
---

# Add subtle fade transition when closing side panels (Diff, Markdown, FileBrowser, Notes)

## Problem Statement

All four side panels (DiffPanel, MarkdownPanel, FileBrowserPanel, NotesPanel) use a hard `display: none` toggle via the `.hidden` CSS class. Closing a panel is visually jarring — the panel disappears in a single frame, causing the terminal to jump sideways abruptly.

## Desired Behavior

When a panel closes, it should fade out with a brief opacity transition (~120-150ms) before being removed from layout. Opening can remain instant or have a matching fade-in. The transition should feel snappy — not slow or floaty.

## Technical Approach

All 4 panels share the same pattern:
- `.panel` class defines layout (flex column, width, border-left)
- `.hidden` class applies `display: none`
- Visibility controlled by `props.visible` boolean

**Problem:** `display: none` cannot be animated with CSS transitions. We need to decouple the opacity fade from the layout removal.

**Solution options:**
1. **CSS-only with `@starting-style` + `transition-behavior: allow-discrete`** — Modern CSS (Chrome 117+, Safari 17.4+). Uses `@starting-style` for entry animation and `allow-discrete` to transition `display`. Tauri WebView supports this on macOS (WebKit) and Windows/Linux (WebView2/Chromium).
2. **JS timer approach** — Keep panel in DOM during fade-out, remove after `transitionend` event. More code, works everywhere.

Option 1 is preferred: the app targets modern WebKit/Chromium via Tauri v2, no legacy browser support needed.

## Affected Files

| File | Change |
|------|--------|
| `src/components/DiffPanel/DiffPanel.module.css` | Replace `.hidden { display: none }` with fade transition |
| `src/components/MarkdownPanel/MarkdownPanel.module.css` | Same |
| `src/components/FileBrowserPanel/FileBrowserPanel.module.css` | Same |
| `src/components/NotesPanel/NotesPanel.module.css` | Same |

Possibly extract shared transition classes into `src/components/shared/panel.module.css` to avoid duplication.

## Acceptance Criteria

- [ ] Closing any side panel plays a ~120-150ms fade-out before disappearing
- [ ] Opening a panel is instant or has a matching quick fade-in
- [ ] Terminal area resizes smoothly (no layout jump before fade completes)
- [ ] No regression in panel toggle speed or keyboard shortcut responsiveness
- [ ] Works on macOS (WebKit) and Windows/Linux (Chromium/WebView2)

## QA

- [ ] .Visually verify fade-in/out on all 4 panels (Cmd+D, Cmd+M, Cmd+E, Cmd+N)

## Work Log

### 2026-02-27T11:28:35.082Z - Added 150ms opacity fade with @starting-style + transition-behavior: allow-discrete to all 4 side panel CSS modules (Diff, Markdown, FileBrowser, Notes). Panel fades out before display:none kicks in, and fades in when opened. CSS-only solution, no JS needed.

