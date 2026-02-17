---
id: 100-9c3a
title: "MD: markdown file browser with rendered preview tabs"
status: complete
priority: P1
created: "2026-02-15T10:29:48.830Z"
updated: "2026-02-15T16:47:18.611Z"
dependencies: []
---

# MD: markdown file browser with rendered preview tabs

## Problem Statement

The MD panel currently tries to load only README.md via a get_readme Tauri command that is NOT implemented in the Rust backend (always errors). It overlays the terminal as a fullscreen panel instead of using a proper sidebar+tab pattern. Users need to browse all markdown files in the repo and open any of them in a dedicated tab with proper rendered preview.

## Acceptance Criteria

- [ ] Add Rust backend command list_markdown_files(path) that finds all .md files in the repo recursively, returning paths relative to repo root
- [ ] Add Rust backend command read_file(path, file) that reads file content for any file path
- [ ] Replace current MarkdownPanel fullscreen overlay with a Markdown Browser sidebar (same pattern as Diff change browser): collapsible tree or flat list of .md files found in repo
- [ ] Clicking a .md file opens it in a new tab with rendered markdown preview
- [ ] Improve MarkdownRenderer: consider using a proper markdown library (e.g. marked or markdown-it) instead of the fragile regex parser, or at minimum fix known gaps in the custom renderer
- [ ] Sidebar shows file tree grouped by directory, with file count badge in header
- [ ] Preserve Cmd+M keyboard shortcut to toggle the markdown browser sidebar
- [ ] Remove the broken get_readme frontend call and replace with the new list+read approach

## Files

- src/components/MarkdownPanel/MarkdownPanel.tsx
- src/components/ui/MarkdownRenderer.tsx
- src/styles.css
- src-tauri/src/lib.rs
- src/hooks/useRepository.ts
- src/stores/ui.ts
- src/App.tsx

## Work Log

### 2026-02-15T16:40:31.166Z - AUTONOMOUS DECISION: Using 'marked' library for markdown rendering instead of fixing custom regex parser. Rationale: (1) More robust and handles edge cases, (2) Widely used and well-maintained, (3) Lightweight (~10KB), (4) Supports all markdown features (links, tables, etc.), (5) Saves development time vs fixing fragile regex approach.

