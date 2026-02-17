---
id: 023-182a
title: Rate limit detector for agents
status: complete
priority: P2
created: "2026-02-04T11:31:14.251Z"
updated: "2026-02-04T12:15:17.402Z"
dependencies: ["033-9a09"]
---

# Rate limit detector for agents

## Problem Statement

When running 50+ agents, rate limits from Claude/OpenAI APIs will occur. Need to detect rate limiting from agent stderr and handle gracefully.

## Acceptance Criteria

- [ ] Detect HTTP 429 and rate-limit patterns in stderr
- [ ] Extract retry-after duration when available
- [ ] Agent-specific patterns for Claude and OpenAI
- [ ] Emit rate-limit event with details
- [ ] Track which agents are currently rate-limited

## Files

- src/rate-limit.ts
- src/main.ts

## Work Log

