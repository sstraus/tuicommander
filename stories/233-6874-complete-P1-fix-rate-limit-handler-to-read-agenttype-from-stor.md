---
id: 233-6874
title: Fix rate-limit handler to read agentType from store instead of props
status: complete
priority: P1
created: "2026-02-17T10:28:43.733Z"
updated: "2026-02-17T10:28:59.019Z"
dependencies: []
---

# Fix rate-limit handler to read agentType from store instead of props

## Problem Statement

Terminal.tsx rate-limit handler was gated on props.agentType which was never passed, making rate limit detection completely non-functional.

## Acceptance Criteria

- [ ] Removed agentType from TerminalProps interface
- [ ] Rate-limit handler reads agentType from terminalsStore.get(id)
- [ ] Removed unused AgentType import from Terminal.tsx

## Files

- src/components/Terminal/Terminal.tsx

## Work Log

### 2026-02-17T10:28:58.912Z - Removed agentType from TerminalProps. Rate-limit handler now reads from terminalsStore.get(id).agentType. Removed unused AgentType import.

