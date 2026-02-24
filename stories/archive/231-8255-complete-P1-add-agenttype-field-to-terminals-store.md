---
id: 231-8255
title: Add agentType field to terminals store
status: complete
priority: P1
created: "2026-02-17T10:28:43.731Z"
updated: "2026-02-17T10:28:58.462Z"
dependencies: []
---

# Add agentType field to terminals store

## Problem Statement

Terminal store has no field to track which AI agent is running in each terminal session.

## Acceptance Criteria

- [ ] agentType: AgentType | null added to TerminalData interface
- [ ] Default value is null
- [ ] Omitted from add() required params (auto-set to null)

## Files

- src/stores/terminals.ts

## Work Log

### 2026-02-17T10:28:58.357Z - Added agentType: AgentType | null to TerminalData interface with null default. Updated add() Omit type to exclude agentType.

