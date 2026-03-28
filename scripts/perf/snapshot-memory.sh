#!/usr/bin/env bash
# Guide for taking frontend memory snapshots.
# This script prints instructions — memory profiling requires manual DevTools interaction.
#
# Usage: scripts/perf/snapshot-memory.sh
cat <<'GUIDE'
=== Frontend Memory Profiling Guide ===

SETUP:
  1. Launch TUICommander in dev mode: npm run tauri dev
  2. Open DevTools: Cmd+Shift+I (or right-click → Inspect)
  3. Go to Memory tab

SCENARIO 1: Terminal Memory Baseline
  a. Take Heap Snapshot (click camera icon) → label "0 terminals"
  b. Open 5 terminal tabs
  c. Run `find /usr -type f` in each (generates scrollback)
  d. Wait for commands to finish
  e. Take Heap Snapshot → label "5 terminals active"
  f. Close all 5 terminals
  g. Click "Collect garbage" (trash icon)
  h. Take Heap Snapshot → label "5 terminals closed"

  Compare snapshots:
  - "5 terminals active" vs "0 terminals": expected ~8-16MB growth
    (xterm scrollback: ~1.6MB × 5 at 10k lines × 80 cols)
  - "5 terminals closed" vs "0 terminals": should be <1MB difference
    If large: memory leak — look at retained objects

SCENARIO 2: Panel Memory Leak Check
  a. Take Heap Snapshot → label "baseline"
  b. Open/close Settings panel 10 times rapidly
  c. Open/close Activity Dashboard 10 times
  d. Open/close Git panel 10 times
  e. Click "Collect garbage"
  f. Take Heap Snapshot → label "after panels"

  Compare: growth should be <500KB. If larger, check for:
  - Event listeners not cleaned up (look for "EventListener" in retained)
  - Timers not cleared (setInterval/setTimeout in retainers)
  - Store subscriptions not disposed

SCENARIO 3: Long-Running Session
  a. Take Heap Snapshot → label "start"
  b. Use the app normally for 10 minutes
     (switch tabs, use git panel, run commands)
  c. Take Heap Snapshot → label "10min"
  d. Continue for another 10 minutes
  e. Take Heap Snapshot → label "20min"

  Check for linear growth between snapshots:
  - Stable or sub-linear growth = healthy
  - Linear growth = likely leak

  Use "Comparison" view to see what's growing.

WHAT TO LOOK FOR:
  - Detached DOM trees (search "Detached" in snapshot)
  - Growing string arrays (terminal output not being GC'd)
  - Unreleased closures (from createEffect without onCleanup)
  - xterm.js WebGL textures not freed on terminal close

RESULTS:
  Export snapshots via right-click → "Save..." for comparison across sessions.
  Save to scripts/perf/results/ for tracking over time.

GUIDE
