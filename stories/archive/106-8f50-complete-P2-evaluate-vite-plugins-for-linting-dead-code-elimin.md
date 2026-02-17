---
id: 106-8f50
title: Evaluate Vite plugins for linting, dead code elimination, and bundle optimization
status: complete
priority: P2
created: "2026-02-15T12:26:34.228Z"
updated: "2026-02-15T17:26:18.529Z"
dependencies: []
---

# Evaluate Vite plugins for linting, dead code elimination, and bundle optimization

## Problem Statement

We have no automated tooling beyond lightningcss for CSS. We should evaluate ESLint (with SolidJS plugin), vite-plugin-checker, vite-plugin-inspect, rollup-plugin-visualizer, and tree-shaking configuration to identify unused exports, dead code paths, and bundle size improvements. This directly impacts load performance in the Tauri WebView.

## Acceptance Criteria

- [ ] Research and document available Vite/Rollup plugins for: ESLint integration, dead code detection, bundle analysis, unused import detection
- [ ] Evaluate each plugin for compatibility with SolidJS + Tauri stack
- [ ] Implement the selected plugins and measure impact on bundle size
- [ ] Document findings and configuration in SPEC.md or a dedicated doc

## Files

- vite.config.ts
- package.json

## Work Log

### 2026-02-15T17:26:18.468Z - Added vite-plugin-checker (TypeScript checking in dev) and rollup-plugin-visualizer (bundle stats on build). Evaluated vite-plugin-inspect (skip - debugging only), knip (skip - needs tuning), eslint-plugin-solid (deferred - needs ESLint config setup). Current bundle: 670KB JS / 182KB gzip.

