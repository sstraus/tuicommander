---
id: 321-a3f0
title: Virtual markdown tab support in mdTabsStore and MarkdownTab
status: ready
priority: P1
created: "2026-02-21T09:34:29.891Z"
updated: "2026-02-21T10:02:49.049Z"
dependencies: ["319-0595"]
---

# Virtual markdown tab support in mdTabsStore and MarkdownTab

## Problem Statement

Currently mdTabsStore only supports file-based tabs (repoPath + filePath). Plugin items need to open dynamically generated markdown content. The store and MarkdownTab component must support a virtual tab type that resolves content via markdownProviderRegistry instead of reading from the filesystem.

## Acceptance Criteria

- [ ] MdTabData becomes a discriminated union: { type: file, repoPath, filePath, fileName } | { type: virtual, id, title, contentUri }
- [ ] mdTabsStore.addVirtual(title, contentUri): creates a virtual tab, returns id
- [ ] addVirtual with same contentUri returns existing tab id (dedup, same as file tab behavior)
- [ ] remove, setActive, clearAll work correctly for both tab types
- [ ] getActive() returns the correct discriminated union type
- [ ] MarkdownTab component branches on tab type: file tabs use repo.readFile, virtual tabs use markdownProviderRegistry.resolve(contentUri)
- [ ] Virtual tab shows loading state while resolving async providers
- [ ] Virtual tab shows error state when provider returns null or throws
- [ ] All existing mdTabs tests still pass

## Files

- src/stores/mdTabs.ts
- src/components/MarkdownTab/MarkdownTab.tsx
- src/__tests__/stores/mdTabs.test.ts

## Related

- 319-0595

## Work Log

