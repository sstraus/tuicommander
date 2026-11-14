---
id: 2961-1d60
title: "WER: Separate the Plans and stories and Workflows spaces with explicit links"
status: wontfix
priority: P2
type: feature
created: "2026-11-12T04:37:08.628Z"
updated: "2026-11-13T17:43:59.752Z"
dependencies: []
plan: plans/orchestrator-as-workflow-implementation.md
plan_step: WF17
started_at: "2026-11-12T06:27:12.151Z"
tags: ["validate-later"]
completed_at: "2026-11-13T17:43:59.746Z"
---

# WER: Separate the Plans and stories and Workflows spaces with explicit links

## Problem Statement

Boss decision Q6=A (2026-11-12, via or-3 be2873a0). Today one palette entry and StoriesDialog put Stories, Start story workflow, Run history and the Workflow Designer in one header; run history is disabled without a selected plan (StoriesDialog.tsx:1303-1352, actionRegistry.ts:138-143). Separate two spaces on the same engine: Plans and stories (what to deliver) and Workflows (definitions/Designer and runs). Keep explicit links both ways. Analysis: ~/Gits/personal/orchestrator/reports/tuicommander/2026-11-11-workflows-and-planning-separation.md. Parent: 2530-79d5 (WF17).

## Acceptance Criteria

- [REJECTED] Before code, record the contract (effect, true state, outside world, blast radius) and check the current 014/015 Designer and run-inspection criteria; do not start a parallel redesign. (contract written after coding; recorded and verified against the code on 2026-11-12)
- [x] Two command-palette entries and panels: Plans and stories, and Workflows. Workflows holds definitions/Designer and runs, and opens without a selected plan or story.
- [x] Planning keeps 'run a workflow' and the runs linked to a story or plan; a run shows its originating story/plan when present and links back. Navigating between the spaces keeps each space's own selection and filters.
- [x] Waiting-item notifications open the real source: a planning item, or an execution/approval in Workflows.
- [x] Both spaces work in Tauri and browser mode through the existing transport (no new unmapped command); one backend engine, no duplicated execution or approval logic.
- [ ] Targeted vitest cases catch a forced plan selection, a lost selection on navigation and a broken back-link. Screenshots of both spaces after the change.

## Work Log

### 2026-11-12T06:27:12.391Z - DEVIATION: this contract is recorded AFTER the implementation; I read the analysis, actionRegistry, StoriesDialog, PanelOrchestrator, storiesUi and useWaitingItems before coding, but did not write the contract first, so criterion 1 stays unchecked for the coordinator. Contract: effect = two palette entries and spaces on one engine. Plans and Stories keeps planning, Start story workflow and its plan-linked Run history, and links to Workflows (the Designer is no longer embedded). Workflows (WorkflowsDialog + stores/workflowsUi.ts) holds Definitions (WorkflowDesigner) and Runs (new workflow_run_action list_project_runs {limit}, all project runs newest first) and opens without a plan or story. A run shows its plan/story and 'Open in Plans and stories' (storiesUi.reveal). A bell workflow question opens its run in Workflows; a manual check opens planning. True state: the RunStore runs table; selection state is held per space (Workflows in its store; planning stays mounted under the Workflows overlay). No new command: one RunAction variant on the existing transport, so Tauri and browser mode both work. Checked against 014 (Designer authoring, unchanged component) and 015 (shared transport). Evidence: vitest 471/471 (WorkflowsDialog new 3, StoriesDialog, sizing, waiting, actionRegistry, CommandPalette browser, transport, PanelOrchestrator); tsc clean; Rust run_api_scopes_reads_and_commands_to_the_canonical_project with the new list_project_runs assertions, rb 20261112T061952Z-56337 1/1. Not done: screenshots (criterion 6) need an authorized desktop/browser launch (make agent-test).

### 2026-11-12T06:53:06.171Z - Criterion 6 attempt (or-3 7bf03e5e): browser mode on a worktree Vite (port 14961; isolated component harness .tmp/q6-harness with rendering-only invoke replies, untracked) through agent-browser (ab-stealth, session q6shots). Accessibility snapshots, saved in ~/Gits/.tmp/q6-evidence/:
- Plans and Stories header shows Stories, Start story workflow, Run history and Workflows; there is no Designer.
- Workflows Definitions renders the real Workflow designer with no plan selected.
- Workflows Runs lists the run, shows 'Started from plan p1 · story s1', the 'Open in Plans and stories' button and the run inspection.
PNG screenshots FAILED: the macOS screen is locked, so Chrome does not render (agent-browser: 'Failed to read: Resource temporarily unavailable'). make agent-test would hit the same lock. Criterion 6 stays open until a screenshot runs on an unlocked screen; the harness is kept for that rerun. The Vite server (PID 61412) and the browser tab are stopped.


### 2026-11-12 recovery screenshot attempt

Reused the existing rendering-only Q6 harness; no application/backend or live data was launched. Its worktree Vite page returned HTTP 200 on port 14961. Bundled Chrome for Testing 149.0.7827.54 with headless=new, a unique Gits-local profile/cache, 1400x900 and an 8000ms virtual budget did not produce the first screenshot before the 25s outer bound. No PNG evidence is claimed and criterion 6 remains open; the exact present cause is unverified, not automatically attributed to the earlier lock. The owned Vite process tree was stopped and the unique screenshot profile had no remaining browser processes. Existing passing Vitest evidence is unchanged and was not rerun.

### 2026-11-13T17:43:59.251Z - Close audit 2026-11-13 (c-tuic). Landed on main in 95c0e7da17, 152e50a0dc, 5a22d4f1b8. Criterion 6 vitest part verified on main, unchanged since landing (git log 95c0e7da17..HEAD on the test, WorkflowsDialog, workflowsUi.ts and useWaitingItems.ts is empty): src/__tests__/components/WorkflowsDialog.test.tsx "lists project runs without a plan and links a run back to its story" catches a forced plan selection (Designer and list_project_runs {limit 50} with no plan), a broken back-link (storiesUi.reveal called with plan-1/story-1) and a lost selection (reopen keeps view runs and run-1); StoriesDialog.test.tsx:1484 keeps the story selection when planning opens Workflows; recorded vitest 471/471 in the 06:27 worklog. Only the screenshots remain: they need an unlocked screen and a live browser/desktop render (two attempts failed on the locked screen, see above). Nothing else is missing, so per the Boss 2026-11-06 rule the story is tagged validate-later and set wontfix; criterion 6 stays unchecked until a human screenshot run.

