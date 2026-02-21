---
id: 319-0595
title: Markdown provider registry for virtual content URIs
status: complete
priority: P1
created: "2026-02-21T09:33:40.202Z"
updated: "2026-02-21T10:10:19.766Z"
dependencies: ["317-af1e"]
---

# Markdown provider registry for virtual content URIs

## Problem Statement

Plugin items need to display dynamically generated markdown content (not read from filesystem). A URI scheme registry (VS Code TextDocumentContentProvider pattern) lets plugins register content generators keyed by scheme, e.g. plan:file?path=... or stories:detail?id=042.

## Acceptance Criteria

- [ ] src/plugins/markdownProviderRegistry.ts exports markdownProviderRegistry singleton
- [ ] register(scheme, provider): adds provider, returns Disposable that removes it
- [ ] resolve(uri): routes to correct provider by URI scheme, returns null for unknown schemes
- [ ] resolve handles both synchronous and async providers
- [ ] URI parsing works for schemes with query params (e.g. plan:file?path=/foo/bar.md)
- [ ] All tests pass including disposal cleanup

## Files

- src/plugins/markdownProviderRegistry.ts
- src/__tests__/plugins/markdownProviderRegistry.test.ts

## Work Log

### 2026-02-21T10:10:19.689Z - Implemented markdownProviderRegistry with per-scheme provider stacks. Stack design means dispose() restores previous provider for same scheme. 14/14 tests green covering sync/async providers, URI parsing, multi-scheme routing, dispose behaviour.

