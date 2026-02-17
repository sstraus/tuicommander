---
id: 028-c0e5
title: Audio notification when agent awaits input
status: pending
priority: P2
created: "2026-02-04T11:31:14.252Z"
updated: "2026-02-04T11:53:03.995Z"
dependencies: ["033-9a09"]
---

# Audio notification when agent awaits input

## Problem Statement

When agent asks a question (AskUserQuestion), user may not notice if working in another window. Need audio cue.

## Acceptance Criteria

- [ ] Play notification sound when agent prompts for input
- [ ] Configurable sound (on/off, volume)
- [ ] Different sounds for: question, error, completion
- [ ] Respect system Do Not Disturb settings
- [ ] Bundle default notification sounds

## Files

- src/notifications.ts
- src/main.ts
- src/assets/sounds/

## Work Log

