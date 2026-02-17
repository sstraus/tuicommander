---
id: 199-664e
title: Add Windows and Linux font fallbacks in CSS
status: complete
priority: P3
created: "2026-02-16T13:20:02.392Z"
updated: "2026-02-16T13:48:44.400Z"
dependencies: []
---

# Add Windows and Linux font fallbacks in CSS

## Problem Statement

CSS font stacks include macOS-only fonts (SF Mono, Menlo, Monaco) but no Windows equivalents. Falls back to generic monospace which may look poor.

## Acceptance Criteria

- [ ] Add Consolas and Cascadia Code to font fallback chains
- [ ] Keep macOS fonts for macOS users
- [ ] Order: bundled fonts then platform fonts then generic monospace

## Files

- src/styles.css

## Work Log

### 2026-02-16T13:48:44.326Z - Added Cascadia Code, Consolas, DejaVu Sans Mono to --font-mono; Noto Sans, Liberation Sans to --font-ui; replaced hardcoded font stack with var(--font-mono)

