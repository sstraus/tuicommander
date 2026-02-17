---
id: 025-2f3e
title: Fallback agent chain
status: pending
priority: P2
created: "2026-02-04T11:31:14.252Z"
updated: "2026-02-04T11:53:03.843Z"
dependencies: ["033-9a09"]
---

# Fallback agent chain

## Problem Statement

When primary agent is rate-limited, need automatic switch to fallback agents with periodic recovery testing.

## Acceptance Criteria

- [ ] Configure primary and fallback agents list
- [ ] Auto-switch to next available agent on rate limit
- [ ] Periodic test if primary agent recovered (minimal prompt)
- [ ] Auto-recovery when primary is available
- [ ] UI indicator showing which agent is active

## Files

- src/agent-manager.ts
- src/main.ts

## Work Log

