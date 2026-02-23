---
id: 136-13bd
title: Build model selector UI with download/delete buttons
status: complete
priority: P1
created: "2026-02-15T22:01:30.929Z"
updated: "2026-02-15T22:42:21.679Z"
dependencies: ["135"]
---

# Build model selector UI with download/delete buttons

## Problem Statement

DictationSettings shows single hardcoded model status. Need model picker UI with per-model download/delete actions.

## Acceptance Criteria

- [ ] Model selector (radio/dropdown) to pick active model
- [ ] Per-model row shows: display name, size hint, status badge
- [ ] Download button for not-downloaded models with progress bar
- [ ] Delete button for downloaded models
- [ ] Active model restricted to downloaded models only
- [ ] refreshModels() called on mount
- [ ] Selecting a model persists to config and shows as active

## Files

- src/components/SettingsPanel/DictationSettings.tsx

## Related

- 003

## Work Log

### 2026-02-15T22:42:21.618Z - Implemented model selector UI with per-model rows, download/delete/select buttons, status badges, progress bar. 12 component tests added.

