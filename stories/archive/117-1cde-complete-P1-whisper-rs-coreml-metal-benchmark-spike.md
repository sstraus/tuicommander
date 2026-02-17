---
id: 117-1cde
title: Whisper-rs CoreML+Metal benchmark spike
status: complete
priority: P1
created: "2026-02-15T14:42:08.916Z"
updated: "2026-02-15T14:54:56.491Z"
dependencies: []
---

# Whisper-rs CoreML fork & benchmark spike

## Problem Statement

whisper-rs does not expose CoreML backend. Need to fork, add CoreML feature flag, and benchmark Metal-only vs CoreML+Metal on Apple Silicon to determine backend strategy before production implementation.

## Acceptance Criteria

- [ ] Fork whisper-rs with CoreML feature flag in sys crate build.rs
- [ ] Standalone benchmark binary comparing CPU vs Metal vs CoreML+Metal
- [ ] Benchmark results documented in plans/whisper-benchmark-results.md
- [ ] Decision: use fork or upstream whisper-rs

## Files

- benchmarks/whisper-bench/
- plans/whisper-benchmark-results.md

## Related

- plans/voice-dictation.md

## Work Log

### 2026-02-15T14:47:14.438Z - No fork needed: whisper-rs 0.15+ already has coreml feature flag. CoreML requires companion .mlmodelc encoder file alongside GGML .bin. Pre-generated CoreML models available on HuggingFace (ggerganov/whisper.cpp repo). Building benchmark with metal vs coreml+metal features.

### 2026-02-15T14:53:16.693Z - Metal benchmark on M4 Max with base model: 5s audio=49ms, 10s=47ms, 30s=52ms. RTF 0.002-0.01 (blazing fast). CoreML+Metal feature compiles successfully. Downloading large-v3-turbo (~1.6GB) for final benchmark.

### 2026-02-15T14:54:56.430Z - large-v3-turbo benchmark complete: 357-369ms on M4 Max with Metal. RTF 0.012-0.071. Decision: Metal-only, no CoreML needed. CoreML compiles but adds complexity for negligible gain at these latencies.

