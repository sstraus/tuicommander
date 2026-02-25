---
id: 379-2b40
title: Commit plugins submodule - registry.json and README cleanup after wiz-* removal
status: complete
priority: P2
created: "2026-02-25T07:45:42.399Z"
updated: "2026-02-25T08:28:30.496Z"
dependencies: []
---

# Commit plugins submodule - registry.json and README cleanup after wiz-* removal

## Problem Statement

session ce8b00ba removed wiz-* from examples/ (commit 40653b9) but plugins/ submodule registry.json and README still have uncommitted local changes. Need to commit and push to sstraus/tuicommander-plugins.

## Acceptance Criteria

- [ ] Commit registry.json in plugins/ submodule (remove wiz-stories, wiz-reviews entries)
- [ ] Commit README changes in plugins/ submodule
- [ ] Push submodule to origin
- [ ] Update submodule pointer in main repo and commit
- [ ] git submodule status shows clean

## Work Log

### 2026-02-25T08:28:30.427Z - Committed registry.json + README + mdkb-dashboard + .gitignore in submodule. Pushed to sstraus/tuicommander-plugins. Submodule pointer updated.

