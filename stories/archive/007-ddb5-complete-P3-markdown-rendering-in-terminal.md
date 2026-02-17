---
id: 007-ddb5
title: Markdown rendering in terminal
status: complete
priority: P3
created: "2026-02-04T10:50:24.107Z"
updated: "2026-02-04T11:31:13.542Z"
dependencies: []
---

# Markdown rendering in terminal

## Problem Statement

Agent output includes markdown (**, ##, lists). Currently displays raw. Should render formatted.

## Acceptance Criteria

- [ ] Parse markdown from ANSI stream
- [ ] Render bold, headers, lists
- [ ] Keep ANSI colors for code blocks
- [ ] Toggle raw/rendered view

## Files

- src/main.ts

## Work Log

