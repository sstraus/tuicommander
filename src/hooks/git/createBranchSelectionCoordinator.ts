import { batch } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { paneLayoutKey, savedPaneLayouts } from "../../stores/savedPaneLayouts";
import { reconcileTerminalOwnership } from "../../stores/terminalOwnership";
import { terminalsStore } from "../../stores/terminals";
import type { SavedTerminal } from "../../types";
import { resolveAgentResume, verifyAndBuildResumeCommand } from "../../utils/agentSession";
import { assignTabToActiveGroup } from "../../utils/paneTabAssign";
import { markPerf } from "../../utils/perfTrace";
import { filterValidTerminals } from "../../utils/terminalFilter";
import { disarmEagerResume, isEagerResumeArmed } from "../../utils/updateRestart";

interface BranchSelectionCoordinatorDeps {
	repo: {
		getDiffStats: (path: string) => Promise<{ additions: number; deletions: number }>;
	};
	pty: {
		canSpawn: () => Promise<boolean>;
	};
	setStatusInfo: (message: string) => void;
	getDefaultFontSize: () => number;
}

/** Owns terminal creation and serialized branch activation. */
export function createBranchSelectionCoordinator(deps: BranchSelectionCoordinatorDeps) {
	let branchSelectQueue: Promise<void> = Promise.resolve();

	/** `workspaceId` is the row the terminal joins. The branch it displays comes
	 *  from that row's record rather than being inferred from the map key. */
	const handleAddTerminalToWorkspace = async (repoPath: string, workspaceId: string) => {
		const canSpawn = await deps.pty.canSpawn();
		if (!canSpawn) {
			deps.setStatusInfo("Max sessions reached (50)");
			return;
		}

		// Ensure this repo+branch is active without going through handleBranchSelect
		// (which has auto-spawn logic that would create a duplicate terminal).
		// Batch all store writes to flush the reactive graph once instead of 6+ times.
		const activeRepo = repositoriesStore.getActive();
		const needsSwitch = activeRepo?.path !== repoPath || activeRepo?.activeWorkspaceId !== workspaceId;

		const branch = repositoriesStore.get(repoPath)?.workspaces[workspaceId];
		if (!branch) {
			// The id names no row: the terminal below would join nothing (so it renders
			// without a tab) and spawn with a null cwd, which the backend reads as the
			// user's HOME. The repo is known here, so neither has to happen.
			appLogger.error("git", `handleAddTerminalToWorkspace: "${workspaceId}" names no workspace`, {
				repoPath,
				known: Object.keys(repositoriesStore.get(repoPath)?.workspaces ?? {}),
			});
		}
		const termCount = branch?.terminals.length || 0;

		const branchName = branch?.branchName ?? workspaceId;
		// DEFERRED (2026-09-11) — branch labels are a branch-keyed config map, so two
		// workspaces on one branch share one label and removing either drops it.
		// Migrating that map is a persisted-shape change in Rust (#728-bc76 kept the
		// frontend key change separate); see the same note at `remove_branch_label`.
		const label = repoSettingsStore.getEffective(repoPath)?.branchLabels?.[branchName];
		const tabName = label ?? `${branchName.split(/[\\/]/).pop()} ${termCount + 1}`;
		const id = terminalsStore.add({
			sessionId: null,
			fontSize: deps.getDefaultFontSize(),
			name: tabName,
			// `null` here is not "the default directory" — the backend spawns in the
			// user's HOME. A workspace of this repo belongs in this repo: a linked
			// worktree has its own path, everything else is the repo checkout.
			cwd: branch?.worktreePath || repoPath,
			awaitingInput: null,
			tuicSession: crypto.randomUUID(),
		});
		if (label) terminalsStore.update(id, { nameIsCustom: true });

		batch(() => {
			if (needsSwitch) {
				repositoriesStore.setActive(repoPath);
				repositoriesStore.setActiveWorkspace(repoPath, workspaceId);
			}
			// The owner of record, not just the display index. This path is handed the
			// repo, so leaving the field null would file a deliberate placement as the
			// "no registered repo claims this cwd" guess reconcileTerminalOwnership
			// is entitled to overturn.
			terminalsStore.setRepoPath(id, repoPath);
			repositoriesStore.addTerminalToWorkspace(repoPath, workspaceId, id);
			terminalsStore.setActive(id);
			if (!needsSwitch) {
				assignTabToActiveGroup(id, "terminal");
			}
		});
		// Focus the new terminal after SolidJS renders and mounts the component
		// (onMount sets ref, which happens in the next frame).
		requestAnimationFrame(() => terminalsStore.get(id)?.ref?.focus());
		return id;
	};

	interface RestoredTab {
		id: string;
		terminal: SavedTerminal;
		/** Set only when the command was resolved up front (post-update restart). */
		resolution: ResumeResolution | null;
	}
	type ResumeResolution = Awaited<ReturnType<typeof resolveAgentResume>>;

	/** Create terminals (no PTY yet) for the saved tabs of one workspace.
	 *  `autoResume` is the post-update restart: resume commands are resolved BEFORE the
	 *  terminals exist, so a tab whose session the agent confirmed gets its command as
	 *  `pendingInitCommand` before its PTY is created and types it at first idle. */
	const restoreSavedTabs = async (
		repoPath: string,
		workspaceId: string,
		tabs: SavedTerminal[],
		autoResume: boolean,
	): Promise<RestoredTab[]> => {
		const resolutions = autoResume
			? await Promise.all(
					tabs.map((terminal) =>
						terminal.suspended
							? null
							: resolveAgentResume(
									terminal.agentType!,
									terminal.cwd,
									terminal.tuicSession,
									terminal.agentSessionId,
									terminal.agentLaunchCommand,
								),
					),
				)
			: null;
		// First pass: create all terminals synchronously (instant UI)
		const restoredIds: RestoredTab[] = [];
		for (const [index, terminal] of tabs.entries()) {
			const id = terminalsStore.add({
				sessionId: null,
				fontSize: terminal.fontSize,
				name: terminal.name,
				cwd: terminal.cwd,
				awaitingInput: null,
				tuicSession: terminal.tuicSession ?? crypto.randomUUID(),
				restoredIdentity: Boolean(terminal.tuicSession),
				agentType: terminal.agentType ?? null,
				agentSessionId: terminal.agentSessionId ?? null,
				agentLaunchCommand: terminal.agentLaunchCommand ?? null,
				// The address other agents already hold. Rust reserves it on
				// PTY create and moves the repo counter past it, so the next
				// fresh tab cannot be handed the same name.
				alias: terminal.alias ?? null,
			});
			// Restored without a PTY; the user resumes it.
			if (terminal.suspended) terminalsStore.update(id, { suspended: true });
			// What the tab was doing before the restart. Carried purely so the
			// resume banner and the Context bar can say it — the next `intent:`
			// or user prompt overwrites both.
			if (terminal.agentIntent || terminal.lastPrompt) {
				terminalsStore.update(id, {
					agentIntent: terminal.agentIntent ?? null,
					lastPrompt: terminal.lastPrompt ?? null,
				});
			}
			const resolution = resolutions?.[index] ?? null;
			if (resolution?.verified && resolution.cmd) terminalsStore.update(id, { pendingInitCommand: resolution.cmd });
			// Same reason as handleAddTerminalToWorkspace: a restore knows its repo.
			terminalsStore.setRepoPath(id, repoPath);
			repositoriesStore.addTerminalToWorkspace(repoPath, workspaceId, id);
			reconcileTerminalOwnership(id);
			restoredIds.push({ id, terminal, resolution });
		}
		return restoredIds;
	};

	/** Second pass: verify resume commands in parallel (non-blocking) and offer the
	 *  click-to-resume banner. A suspended tab stays suspended; resuming it builds the
	 *  command then. A tab already given its init command has nothing left to offer. */
	const applyResumeBanners = (restored: RestoredTab[]): Promise<void> =>
		Promise.all(
			restored
				.filter(({ terminal }) => !terminal.suspended)
				.map(async ({ id, terminal, resolution }) => {
					if (resolution?.verified && resolution.cmd) return;
					const resumeCmd = resolution
						? resolution.cmd
						: await verifyAndBuildResumeCommand(
								terminal.agentType!,
								terminal.cwd,
								terminal.tuicSession,
								terminal.agentSessionId,
								terminal.agentLaunchCommand,
							);
					if (resumeCmd) {
						terminalsStore.update(id, {
							pendingResumeCommand: resumeCmd,
							agentSessionId: terminal.agentSessionId ?? null,
						});
					}
				}),
		)
			.then(() => {})
			.catch((e) => appLogger.warn("terminal", "Resume command verification failed", { error: String(e) }));

	const handleBranchSelect = (repoPath: string, workspaceId: string): Promise<void> => {
		// Append to the FIFO queue: this select runs only after every previously
		// queued select has settled. Each caller awaits the returned promise and sees
		// its own result/rejection; the queue tail swallows rejections so one failed
		// select doesn't break serialization for the calls behind it.
		const run = branchSelectQueue.then(() => handleBranchSelectInner(repoPath, workspaceId));
		branchSelectQueue = run.then(
			() => {},
			() => {},
		);
		return run;
	};

	const handleBranchSelectInner = async (repoPath: string, workspaceId: string) => {
		// Freeze-investigation: repo/branch switch is the reported foreground-freeze
		// trigger. Breadcrumb so a main-thread block during the switch cascade
		// attributes here (the freeze detector reports the freshest crumb).
		markPerf("branch.select", { repoPath, workspaceId });
		// Auto-deactivate global workspace before branch switch
		if (globalWorkspaceStore.isActive()) {
			const prevRepoPath = repositoriesStore.state.activeRepoPath;
			const prevBranch = prevRepoPath ? repositoriesStore.state.repositories[prevRepoPath]?.activeWorkspaceId : null;
			const key = prevRepoPath && prevBranch ? paneLayoutKey(prevRepoPath, prevBranch) : undefined;
			globalWorkspaceStore.deactivate(key);
		}

		repositoriesStore.setBranchSwitching(true);
		try {
			// Log the state we're LEAVING — critical for diagnosing terminal disappearance
			const prevRepo = repositoriesStore.getActive();
			const prevBranchName = prevRepo?.activeWorkspaceId;
			const prevBranch = prevBranchName ? prevRepo?.workspaces[prevBranchName] : null;
			appLogger.debug(
				"terminal",
				`BranchSelect ${prevBranchName ?? "(none)"} → ${workspaceId} terms=${(prevBranch?.terminals ?? []).length}→?`,
			);

			// Save state for the branch we're leaving
			if (prevRepo?.activeWorkspaceId) {
				const currentActiveId = terminalsStore.state.activeId;
				if (currentActiveId && prevBranch?.terminals.includes(currentActiveId)) {
					repositoriesStore.setWorkspace(prevRepo.path, prevRepo.activeWorkspaceId, {
						lastActiveTerminal: currentActiveId,
					});
				}
				// Save pane layout for the branch we're leaving
				if (paneLayoutStore.isSplit()) {
					const key = paneLayoutKey(prevRepo.path, prevRepo.activeWorkspaceId);
					savedPaneLayouts.set(key, paneLayoutStore.serialize());
				} else {
					// Clear any stale layout if user unsplit while on this branch
					savedPaneLayouts.delete(paneLayoutKey(prevRepo.path, prevRepo.activeWorkspaceId));
				}
			}

			// Batch all reactive updates so downstream effects (file browser, etc.)
			// see a consistent snapshot — prevents stale intermediate states where
			// repoPath updated but fsRoot still points to the old worktree.
			batch(() => {
				repositoriesStore.setActive(repoPath);
				repositoriesStore.setActiveWorkspace(repoPath, workspaceId);
			});

			// Fire-and-forget: diff stats are cosmetic, don't block branch switch
			const selectedBranch = repositoriesStore.get(repoPath)?.workspaces[workspaceId];
			if (selectedBranch?.worktreePath) {
				const wtPath = selectedBranch.worktreePath;
				deps.repo
					.getDiffStats(wtPath)
					.then((stats) => {
						repositoriesStore.updateWorkspaceStats(repoPath, workspaceId, stats.additions, stats.deletions);
					})
					.catch((err) => appLogger.debug("git", `getDiffStats failed for ${workspaceId}`, err));
			}
			let branch = repositoriesStore.get(repoPath)?.workspaces[workspaceId];

			// Adopt orphaned terminals whose cwd matches this branch's worktree path.
			// Pre-compute claimed set O(B×T) once, then check in O(1) per terminal.
			if (branch?.worktreePath) {
				const branchTermSet = new Set(branch.terminals);
				const claimedIds = new Set<string>();
				// "Claimed by another ROW", compared by key. Comparing `b.branchName`
				// against the id let a same-branch sibling look like the row itself, so
				// its terminals were not treated as claimed and this select would adopt
				// them out from under it.
				for (const [otherId, other] of Object.entries(repositoriesStore.get(repoPath)?.workspaces ?? {})) {
					if (otherId !== workspaceId) {
						for (const tid of other.terminals) claimedIds.add(tid);
					}
				}
				for (const id of terminalsStore.getIds()) {
					if (branchTermSet.has(id)) continue;
					if (claimedIds.has(id)) continue;
					const term = terminalsStore.get(id);
					if (term?.cwd === branch.worktreePath) {
						repositoriesStore.addTerminalToWorkspace(repoPath, workspaceId, id);
					}
				}
				// Re-read branch state after potential adoptions
				branch = repositoriesStore.get(repoPath)?.workspaces[workspaceId];
			}
			const validTerminals = filterValidTerminals(branch?.terminals, terminalsStore.getIds()).filter(
				(id) => !terminalsStore.isDetached(id),
			);
			appLogger.debug(
				"terminal",
				`BranchSelect → ${workspaceId} valid=${validTerminals.length} saved=${branch?.savedTerminals?.length ?? 0}`,
			);
			if (validTerminals.length === 0 && (branch?.terminals?.length ?? 0) > 0) {
				appLogger.warn(
					"terminal",
					`BranchSelect MISMATCH: branch has terminals ${JSON.stringify(branch?.terminals)} but none found in store ${JSON.stringify(terminalsStore.getIds())}. Will create fresh terminal.`,
				);
			}

			if (validTerminals.length > 0) {
				// Restore saved pane layout if available and all its terminals are still valid
				const layoutKey = paneLayoutKey(repoPath, workspaceId);
				const savedLayout = savedPaneLayouts.get(layoutKey);
				if (savedLayout) {
					const validSet = new Set(validTerminals);
					const layoutTerminals = Object.values(savedLayout.groups).flatMap((g) =>
						g.tabs.filter((t) => t.type === "terminal").map((t) => t.id),
					);
					const allValid = layoutTerminals.length > 0 && layoutTerminals.every((id) => validSet.has(id));
					if (allValid) {
						paneLayoutStore.restore(savedLayout);
					} else {
						savedPaneLayouts.delete(layoutKey);
						paneLayoutStore.reset();
					}
				} else if (paneLayoutStore.consumeRestoredFromDisk()) {
					// Layout was loaded from disk at startup — keep it if terminal IDs are still valid
					const currentLayout = paneLayoutStore.serialize();
					const validSet = new Set(validTerminals);
					const layoutTerminals = Object.values(currentLayout.groups).flatMap((g) =>
						g.tabs.filter((t) => t.type === "terminal").map((t) => t.id),
					);
					if (!(layoutTerminals.length > 0 && layoutTerminals.every((id) => validSet.has(id)))) {
						paneLayoutStore.reset();
					}
				} else {
					paneLayoutStore.reset();
				}
				// Prefer a terminal that is awaiting input (question/error), then lastActive, then first
				const awaitingId = validTerminals.find((id) => terminalsStore.get(id)?.awaitingInput);
				if (awaitingId) {
					terminalsStore.setActive(awaitingId);
				} else {
					const remembered = branch?.lastActiveTerminal;
					if (remembered && validTerminals.includes(remembered)) {
						terminalsStore.setActive(remembered);
					} else {
						terminalsStore.setActive(validTerminals[0]);
					}
				}
			} else if (branch?.savedTerminals && branch.savedTerminals.length > 0) {
				// Only restore agent tabs with resumable sessions — plain shell tabs
				// have nothing meaningful to resume and would just be empty shells.
				// A suspended tab is the exception: the user kept it on purpose.
				const restorableTerminals = branch.savedTerminals.filter((t) => t.agentType != null || t.suspended);
				// Clear savedTerminals (consume-once) regardless of filter result
				repositoriesStore.setWorkspace(repoPath, workspaceId, { savedTerminals: [] });

				if (restorableTerminals.length > 0) {
					// Capture old terminal IDs from the pane layout (branch.terminals is cleared on hydration)
					const oldTerminalIds = paneLayoutStore.getTerminalTabIds();
					// Lazy restore: create terminals from persisted session state
					const hadTerminals = branch.hadTerminals;
					const restoredIds = await restoreSavedTabs(repoPath, workspaceId, restorableTerminals, isEagerResumeArmed());
					// Reconciliation may have moved stale snapshots to another workspace.
					// Only this workspace's tabs may become active or enter its pane layout.
					const ownedIds = new Set(repositoriesStore.get(repoPath)?.workspaces[workspaceId]?.terminals ?? []);
					const selectedRestored = restoredIds.filter(({ id }) => ownedIds.has(id));
					terminalsStore.setActive(selectedRestored[0]?.id ?? null);

					// Keep positional correspondence with the original snapshot, not the filtered list.
					const hasDiskLayout = paneLayoutStore.consumeRestoredFromDisk();
					const idMap = new Map<string, string>();
					for (let i = 0; i < Math.min(oldTerminalIds.length, restoredIds.length); i++) {
						if (ownedIds.has(restoredIds[i].id)) idMap.set(oldTerminalIds[i], restoredIds[i].id);
					}
					if (hasDiskLayout && oldTerminalIds.length > 0 && idMap.size === oldTerminalIds.length) {
						paneLayoutStore.remapTerminalIds(idMap);
						appLogger.debug("terminal", `BranchSelect REMAP disk-restored paneLayout for ${workspaceId}`, {
							remapped: idMap.size,
						});
					} else {
						paneLayoutStore.reset();
					}
					if (selectedRestored.length === 0 && !hadTerminals) {
						await handleAddTerminalToWorkspace(repoPath, workspaceId);
					}

					// Second pass: verify resume commands in parallel (non-blocking).
					// A suspended tab stays suspended; resuming it builds the command then.
					void applyResumeBanners(restoredIds);
				} else {
					// All saved tabs were plain shells — spawn a fresh terminal
					paneLayoutStore.reset();
					await handleAddTerminalToWorkspace(repoPath, workspaceId);
				}
			} else if (!branch?.hadTerminals) {
				// First time selecting this branch — auto-spawn a terminal
				// DEFERRED (2026-07-31) — no existence check on branch.worktreePath: selecting
				// a row whose worktree directory is gone spawns a terminal in a missing cwd
				// ("Spawn failed (dir missing)" + a failed dir watcher), which is what kept
				// deleted worktrees looking alive in the sidebar. The row itself is now pruned
				// at the source (worktree-removed event + worktree set in the repo-watcher
				// fingerprint), so the only way to reach this is a row hydrated from disk
				// before the first refresh prunes it. Needs a backend path-exists round-trip
				// on every branch select — not worth it until that window is observed.
				paneLayoutStore.reset();
				await handleAddTerminalToWorkspace(repoPath, workspaceId);
			} else {
				// hadTerminals && no valid terminals → user closed them all, show empty state.
				// Clear layout and activeId so the previous branch's split doesn't bleed through.
				paneLayoutStore.reset();
				terminalsStore.setActive(null);
			}

			requestAnimationFrame(() => {
				requestAnimationFrame(() => {
					terminalsStore.getActive()?.ref?.focus();
				});
			});
		} finally {
			// Story 1281-a37d: always clear the flag, even on throw. Without this,
			// a rejected close_pty / getDiffStats / resume-verification left the
			// TabBar filtering on the previous repo until app restart.
			repositoriesStore.setBranchSwitching(false);
		}
	};

	/** After an update restart, bring back every branch's agent tabs that have a session
	 *  the agent confirmed — not only the branch the user selects. A branch with no such
	 *  tab keeps its saved tabs for the lazy path. Runs behind pending selects. */
	const resumeAgentTabsAfterUpdate = (): Promise<void> => {
		const run = branchSelectQueue.then(async () => {
			try {
				if (!isEagerResumeArmed()) return;
				for (const repoPath of repositoriesStore.getPaths()) {
					for (const [workspaceId, workspace] of Object.entries(repositoriesStore.get(repoPath)?.workspaces ?? {})) {
						const saved = workspace.savedTerminals ?? [];
						if (workspace.terminals.length > 0) continue;
						if (!saved.some((tab) => tab.agentType != null && !tab.suspended && tab.agentSessionId)) continue;
						// Consume-once, as in a branch select.
						repositoriesStore.setWorkspace(repoPath, workspaceId, { savedTerminals: [] });
						const restored = await restoreSavedTabs(
							repoPath,
							workspaceId,
							saved.filter((tab) => tab.agentType != null || tab.suspended),
							true,
						);
						await applyResumeBanners(restored);
					}
				}
			} finally {
				disarmEagerResume();
			}
		});
		branchSelectQueue = run.then(
			() => {},
			() => {},
		);
		return run;
	};

	return { handleAddTerminalToWorkspace, handleBranchSelect, handleBranchSelectInner, resumeAgentTabsAfterUpdate };
}
