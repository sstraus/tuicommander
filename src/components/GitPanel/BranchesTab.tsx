import { Component, createEffect, createMemo, createSignal, For, Show, on, onCleanup } from "solid-js";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { appLogger } from "../../stores/appLogger";
import { ConfirmDialog } from "../ConfirmDialog";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import { ContextMenu, createContextMenu, type ContextMenuItem } from "../ContextMenu/ContextMenu";
import { cx } from "../../utils";
import { handleOpenUrl } from "../../utils/openUrl";
import type { BranchDetail } from "./types";
import type { BaseRefOption } from "../../hooks/useRepository";
import s from "./BranchesTab.module.css";

/** Convert a git remote URL (SSH or HTTPS) to a GitHub web URL, or null if not GitHub. */
export function remoteUrlToGitHub(remoteUrl: string): string | null {
  // SSH: git@github.com:owner/repo.git
  const sshMatch = remoteUrl.match(/^git@github\.com:(.+?)(?:\.git)?$/);
  if (sshMatch) return `https://github.com/${sshMatch[1]}`;
  // HTTPS: https://github.com/owner/repo.git
  const httpsMatch = remoteUrl.match(/^https?:\/\/github\.com\/(.+?)(?:\.git)?$/);
  if (httpsMatch) return `https://github.com/${httpsMatch[1]}`;
  return null;
}

export interface BranchesTabProps {
  repoPath: string | null;
}

const CheckIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
    <path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z" />
  </svg>
);

const FolderIcon = () => (
  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
    <path d="M1.75 1A1.75 1.75 0 0 0 0 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0 0 16 13.25v-8.5A1.75 1.75 0 0 0 14.25 3H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 1.26 5.55 1 5 1H1.75z" />
  </svg>
);

/** Convert an ISO date string to a short relative label like "3d ago", "2mo ago" */
function relativeDate(isoDate: string | null): string {
  if (!isoDate) return "";
  const then = new Date(isoDate).getTime();
  if (isNaN(then)) return "";
  const diffMs = Date.now() - then;
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
  if (diffDays < 1) return "today";
  if (diffDays < 7) return `${diffDays}d ago`;
  const diffWeeks = Math.floor(diffDays / 7);
  if (diffWeeks < 5) return `${diffWeeks}w ago`;
  const diffMonths = Math.floor(diffDays / 30);
  if (diffMonths < 12) return `${diffMonths}mo ago`;
  const diffYears = Math.floor(diffDays / 365);
  return `${diffYears}y ago`;
}

/** A branch is considered stale if its last commit is older than 30 days */
function isStale(isoDate: string | null): boolean {
  if (!isoDate) return false;
  const then = new Date(isoDate).getTime();
  if (isNaN(then)) return false;
  return Date.now() - then > 30 * 24 * 60 * 60 * 1000;
}

/** Build a multi-line tooltip string with full branch commit info */
function branchTooltip(branch: BranchDetail): string {
  const parts: string[] = [branch.name];
  if (branch.last_commit_date) {
    parts.push(`Last commit: ${new Date(branch.last_commit_date).toLocaleDateString()}`);
  }
  if (branch.last_commit_author) {
    parts.push(`Author: ${branch.last_commit_author}`);
  }
  if (branch.last_commit_message) {
    parts.push(`Message: ${branch.last_commit_message}`);
  }
  if (branch.is_merged) parts.push("(merged)");
  return parts.join("\n");
}

/** A group of branches sharing a common prefix (text before the first "/") */
interface BranchGroup {
  prefix: string;
  branches: BranchDetail[];
}

/**
 * Split a branch list into ungrouped (no "/") and prefix groups.
 * Groups are sorted alphabetically; ungrouped branches come first.
 */
function groupBranchesByPrefix(branchList: BranchDetail[]): { ungrouped: BranchDetail[]; groups: BranchGroup[] } {
  const ungrouped: BranchDetail[] = [];
  const groupMap = new Map<string, BranchDetail[]>();

  for (const b of branchList) {
    const slashIdx = b.name.indexOf("/");
    if (slashIdx === -1) {
      ungrouped.push(b);
    } else {
      const prefix = b.name.slice(0, slashIdx + 1);
      let list = groupMap.get(prefix);
      if (!list) { list = []; groupMap.set(prefix, list); }
      list.push(b);
    }
  }

  const groups: BranchGroup[] = Array.from(groupMap.entries())
    .map(([prefix, branches]) => ({ prefix, branches }))
    .sort((a, b) => a.prefix.localeCompare(b.prefix));

  return { ungrouped, groups };
}

// --- Dialog state types ---

/** Which confirm dialog is open, and what action it performs */
type DialogKind =
  | { type: "delete"; branch: BranchDetail }
  | { type: "merge"; branch: BranchDetail; currentBranch: string }
  | { type: "rebase"; branch: BranchDetail; currentBranch: string };

/** Checkout dirty-worktree state: need to choose stash/force/cancel */
interface DirtyCheckoutState {
  branchName: string;
  isRemote: boolean;
}

/** Create-branch form state */
interface CreateBranchState {
  name: string;
  checkout: boolean;
  startPoint: string | null;
}

export const BranchesTab: Component<BranchesTabProps> = (props) => {
  const [branches, setBranches] = createSignal<BranchDetail[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [recentNames, setRecentNames] = createSignal<string[]>([]);
  const [recentExpanded, setRecentExpanded] = createSignal(true);
  const [search, setSearch] = createSignal("");
  const [localExpanded, setLocalExpanded] = createSignal(true);
  const [remoteExpanded, setRemoteExpanded] = createSignal(true);

  // Prefix folding: group branches by prefix (e.g. "feature/") when enabled
  const [foldingEnabled, setFoldingEnabled] = createSignal(true);
  // Set of group keys that are collapsed. Key format: "local:<prefix>" or "remote:<prefix>"
  const [collapsedGroups, setCollapsedGroups] = createSignal<Set<string>>(new Set());

  // Context menu state
  const ctxMenu = createContextMenu();
  const [ctxBranch, setCtxBranch] = createSignal<BranchDetail | null>(null);

  // Keyboard navigation
  const [selectedIndex, setSelectedIndex] = createSignal(-1);

  // Rename inline state: index into flatVisible list, or -1 when not active
  const [renamingIndex, setRenamingIndex] = createSignal(-1);
  const [renameValue, setRenameValue] = createSignal("");

  // Create-branch inline form
  const [creating, setCreating] = createSignal(false);
  const [createState, setCreateState] = createSignal<CreateBranchState>({ name: "", checkout: true, startPoint: null });
  const [createBaseRefs, setCreateBaseRefs] = createSignal<BaseRefOption[]>([]);

  // Dirty-worktree checkout dialog
  const [dirtyCheckout, setDirtyCheckout] = createSignal<DirtyCheckoutState | null>(null);

  // Generic confirm dialog state
  const [dialog, setDialog] = createSignal<DialogKind | null>(null);

  // Container ref for focus management
  let containerRef: HTMLDivElement | undefined;
  let searchInputRef: HTMLInputElement | undefined;
  let renameInputRef: HTMLInputElement | undefined;
  let createInputRef: HTMLInputElement | undefined;

  async function fetchBranches(repoPath: string) {
    setLoading(true);
    try {
      const [result, recent] = await Promise.all([
        invoke<BranchDetail[]>("get_branches_detail", { path: repoPath }),
        invoke<string[]>("get_recent_branches", { path: repoPath, limit: 5 }).catch((err) => {
          appLogger.warn("git", "Failed to load recent branches", err);
          return [] as string[];
        }),
      ]);
      setBranches(result);
      setRecentNames(recent);
    } catch (err) {
      appLogger.error("git", "Failed to load branches", err);
      setBranches([]);
      setRecentNames([]);
    } finally {
      setLoading(false);
    }
  }

  // Re-fetch when repo changes or revision bumps
  createEffect(
    on(
      () => {
        const repoPath = props.repoPath;
        const rev = repoPath ? repositoriesStore.getRevision(repoPath) : 0;
        return `${repoPath ?? ""}:${rev}`;
      },
      () => {
        const repoPath = props.repoPath;
        if (repoPath) void fetchBranches(repoPath);
        else setBranches([]);
      },
    ),
  );

  // Reset selection only when the active repo changes
  createEffect(
    on(
      () => props.repoPath,
      () => {
        setSelectedIndex(-1);
        setRenamingIndex(-1);
        setCreating(false);
      },
    ),
  );

  const localBranches = createMemo(() =>
    branches().filter((b) => !b.is_remote),
  );

  /** Recent branches resolved to BranchDetail objects (only local, matching recent reflog names) */
  const recentBranches = createMemo(() => {
    const names = recentNames();
    if (names.length === 0) return [];
    const byName = new Map(localBranches().map((b) => [b.name, b]));
    return names
      .map((name) => byName.get(name))
      .filter((b): b is BranchDetail => b !== undefined);
  });

  const remoteBranches = createMemo(() =>
    branches().filter((b) => b.is_remote),
  );

  /** Filter branches by search query, but always keep the current branch visible in local */
  const filteredLocal = createMemo(() => {
    const q = search().trim().toLowerCase();
    if (!q) return localBranches();
    return localBranches().filter(
      (b) => b.is_current || b.name.toLowerCase().includes(q),
    );
  });

  const filteredRemote = createMemo(() => {
    const q = search().trim().toLowerCase();
    if (!q) return remoteBranches();
    return remoteBranches().filter((b) => b.name.toLowerCase().includes(q));
  });

  /** Flat list of all visible branches for keyboard navigation */
  const flatVisible = createMemo((): BranchDetail[] => {
    const items: BranchDetail[] = [];
    if (localExpanded()) items.push(...filteredLocal());
    if (remoteExpanded()) items.push(...filteredRemote());
    return items;
  });

  const currentBranch = createMemo(() =>
    branches().find((b) => b.is_current && !b.is_remote),
  );

  /** Lookup map for O(1) flat index resolution by branch identity */
  const flatIndexMap = createMemo(() => {
    const map = new Map<string, number>();
    flatVisible().forEach((b, i) => map.set(`${b.name}:${b.is_remote}`, i));
    return map;
  });

  // --- Checkout ---

  async function doCheckout(branch: BranchDetail, stash = false, force = false) {
    if (!props.repoPath) return;
    try {
      if (branch.is_remote) {
        const localName = branch.name.replace(/^origin\//, "");
        await invoke("checkout_remote_branch", { repoPath: props.repoPath, branchName: localName });
      } else {
        await invoke("switch_branch", {
          repoPath: props.repoPath,
          branchName: branch.name,
          force,
          stash,
        });
      }
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Switched to branch: ${branch.name}`);
    } catch (err) {
      const errStr = typeof err === "string" ? err : String(err);
      if (!force && !stash && errStr.toLowerCase().includes("dirty")) {
        setDirtyCheckout({ branchName: branch.name, isRemote: branch.is_remote });
      } else {
        appLogger.error("git", `Failed to switch to ${branch.name}`, err);
      }
    }
  }

  async function handleCheckout(branch: BranchDetail) {
    if (!props.repoPath) return;
    if (branch.is_current) return;
    await doCheckout(branch);
  }

  async function handleDirtyCheckoutStash() {
    const state = dirtyCheckout();
    if (!state || !props.repoPath) return;
    setDirtyCheckout(null);
    const branch = branches().find((b) => b.name === state.branchName);
    if (!branch) return;
    await doCheckout(branch, true, false);
  }

  async function handleDirtyCheckoutForce() {
    const state = dirtyCheckout();
    if (!state || !props.repoPath) return;
    setDirtyCheckout(null);
    const branch = branches().find((b) => b.name === state.branchName);
    if (!branch) return;
    await doCheckout(branch, false, true);
  }

  // --- Create Branch ---

  function startCreate() {
    setCreating(true);
    setCreateState({ name: "", checkout: true, startPoint: null });
    // Fetch base ref options for the "from" dropdown
    if (props.repoPath) {
      invoke<BaseRefOption[]>("list_base_ref_options", { repoPath: props.repoPath })
        .then(setCreateBaseRefs)
        .catch(() => setCreateBaseRefs([]));
    }
    // Focus input on next tick
    requestAnimationFrame(() => createInputRef?.focus());
  }

  async function doCreateBranch() {
    if (!props.repoPath) return;
    const state = createState();
    const name = state.name.trim();
    if (!name) {
      setCreating(false);
      return;
    }
    setCreating(false);
    try {
      await invoke("create_branch", {
        path: props.repoPath,
        name,
        startPoint: state.startPoint,
        checkout: state.checkout,
      });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Created branch: ${name}`);
    } catch (err) {
      appLogger.error("git", `Failed to create branch ${name}`, err);
    }
  }

  function cancelCreate() {
    setCreating(false);
    setCreateState({ name: "", checkout: true, startPoint: null });
    setCreateBaseRefs([]);
  }

  // --- Delete Branch ---

  function startDelete(branch: BranchDetail) {
    if (!props.repoPath) return;
    if (branch.is_current) {
      appLogger.warn("git", "Cannot delete the currently checked-out branch");
      return;
    }
    if (branch.is_main) {
      appLogger.warn("git", "Cannot delete the main branch");
      return;
    }
    setDialog({ type: "delete", branch });
  }

  async function doDeleteBranch(force: boolean) {
    const d = dialog();
    if (!d || d.type !== "delete" || !props.repoPath) return;
    const { branch } = d;
    setDialog(null);
    try {
      await invoke("delete_branch", { path: props.repoPath, name: branch.name, force });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Deleted branch: ${branch.name}`);
      setSelectedIndex(-1);
    } catch (err) {
      appLogger.error("git", `Failed to delete branch ${branch.name}`, err);
    }
  }

  // --- Rename Branch ---

  function startRename(branch: BranchDetail, index: number) {
    setRenamingIndex(index);
    setRenameValue(branch.name);
    requestAnimationFrame(() => renameInputRef?.select());
  }

  async function doRenameBranch() {
    if (!props.repoPath) return;
    const idx = renamingIndex();
    if (idx < 0) return;
    const flat = flatVisible();
    const branch = flat[idx];
    if (!branch) { setRenamingIndex(-1); return; }
    const newName = renameValue().trim();
    setRenamingIndex(-1);
    if (!newName || newName === branch.name) return;
    try {
      await invoke("rename_branch", { path: props.repoPath, oldName: branch.name, newName });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Renamed branch ${branch.name} to ${newName}`);
    } catch (err) {
      appLogger.error("git", `Failed to rename branch ${branch.name}`, err);
    }
  }

  function cancelRename() {
    setRenamingIndex(-1);
    setRenameValue("");
  }

  // --- Merge ---

  function startMerge(branch: BranchDetail) {
    if (!props.repoPath) return;
    const cur = currentBranch();
    if (!cur) return;
    if (branch.is_current) {
      appLogger.warn("git", "Cannot merge the current branch into itself");
      return;
    }
    setDialog({ type: "merge", branch, currentBranch: cur.name });
  }

  async function doMerge() {
    const d = dialog();
    if (!d || d.type !== "merge" || !props.repoPath) return;
    const { branch } = d;
    setDialog(null);
    try {
      await invoke("run_git_command", { path: props.repoPath, args: ["merge", branch.name] });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Merged ${branch.name} into ${currentBranch()?.name ?? "current"}`);
    } catch (err) {
      appLogger.error("git", `Merge of ${branch.name} failed (possible conflict)`, err);
      repositoriesStore.bumpRevision(props.repoPath!);
    }
  }

  // --- Rebase ---

  function startRebase(branch: BranchDetail) {
    if (!props.repoPath) return;
    const cur = currentBranch();
    if (!cur) return;
    if (branch.is_current) {
      appLogger.warn("git", "Cannot rebase current branch onto itself");
      return;
    }
    setDialog({ type: "rebase", branch, currentBranch: cur.name });
  }

  async function doRebase() {
    const d = dialog();
    if (!d || d.type !== "rebase" || !props.repoPath) return;
    const { branch } = d;
    setDialog(null);
    try {
      await invoke("run_git_command", { path: props.repoPath, args: ["rebase", branch.name] });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Rebased current branch onto ${branch.name}`);
    } catch (err) {
      appLogger.error("git", `Rebase onto ${branch.name} failed`, err);
      repositoriesStore.bumpRevision(props.repoPath!);
    }
  }

  // --- Push / Pull / Fetch ---

  async function doPush(branch: BranchDetail) {
    if (!props.repoPath) return;
    try {
      const args = branch.upstream
        ? ["push"]
        : ["push", "-u", "origin", branch.name];
      await invoke("run_git_command", { path: props.repoPath, args });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Pushed ${branch.name}`);
    } catch (err) {
      appLogger.error("git", `Push failed for ${branch.name}`, err);
    }
  }

  async function doPull(branch: BranchDetail) {
    if (!props.repoPath) return;
    if (!branch.is_current) {
      appLogger.warn("git", "Pull only works for the currently checked-out branch");
      return;
    }
    try {
      await invoke("run_git_command", { path: props.repoPath, args: ["pull"] });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Pulled ${branch.name}`);
    } catch (err) {
      appLogger.error("git", `Pull failed for ${branch.name}`, err);
    }
  }

  async function openBranchOnGitHub(branch: BranchDetail) {
    if (!props.repoPath) return;
    try {
      const remoteUrl = await invoke<string | null>("get_remote_url", { path: props.repoPath });
      if (!remoteUrl) return;
      const ghBase = remoteUrlToGitHub(remoteUrl);
      if (!ghBase) return;
      // Strip remote prefix: "origin/feature-branch" → "feature-branch"
      const branchName = branch.name.replace(/^[^/]+\//, "");
      handleOpenUrl(`${ghBase}/tree/${encodeURIComponent(branchName)}`);
    } catch (err) {
      appLogger.error("git", "Failed to open branch on GitHub", err);
    }
  }

  async function doFetch(branch: BranchDetail) {
    if (!props.repoPath) return;
    try {
      await invoke("run_git_command", { path: props.repoPath, args: ["fetch", "origin", branch.name] });
      repositoriesStore.bumpRevision(props.repoPath);
      appLogger.info("git", `Fetched ${branch.name}`);
    } catch (err) {
      appLogger.error("git", `Fetch failed for ${branch.name}`, err);
    }
  }

  // --- Compare branches ---

  /** Compare selected branch against the current branch; logs file-level diff summary */
  async function doCompare(branch: BranchDetail) {
    if (!props.repoPath) return;
    const cur = currentBranch();
    if (!cur) {
      appLogger.warn("git", "No current branch to compare against");
      return;
    }
    try {
      const result = await invoke<{ success: boolean; stdout: string; stderr: string }>(
        "run_git_command",
        { path: props.repoPath, args: ["diff", "--name-status", `${cur.name}...${branch.name}`] },
      );
      if (result.success) {
        const lines = result.stdout.trim().split("\n").filter(Boolean);
        if (lines.length === 0) {
          appLogger.info("git", `Compare ${cur.name}...${branch.name}: no differences`);
        } else {
          appLogger.info("git", `Compare ${cur.name}...${branch.name}: ${lines.length} file(s) differ`, { files: lines });
        }
      } else {
        appLogger.warn("git", `Compare failed: ${result.stderr.trim().split("\n")[0]}`);
      }
    } catch (err) {
      appLogger.error("git", `Compare failed for ${branch.name}`, err);
    }
  }

  // --- Prefix group toggle ---

  function toggleGroup(key: string) {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  // --- Update from base ---

  async function doUpdateFromBase(branch: BranchDetail) {
    if (!props.repoPath) return;
    try {
      const result = await invoke<string>("update_from_base", {
        path: props.repoPath,
        branchName: branch.name,
        strategy: "rebase",
      });
      appLogger.info("git", result);
      repositoriesStore.bumpRevision(props.repoPath);
    } catch (err) {
      appLogger.error("git", `Update from base failed for ${branch.name}`, err);
    }
  }

  // --- Context menu ---

  function openContextMenu(e: MouseEvent, branch: BranchDetail) {
    setCtxBranch(branch);
    ctxMenu.open(e);
  }

  function buildContextMenuItems(branch: BranchDetail): ContextMenuItem[] {
    const cur = currentBranch();
    const isCurrent = branch.is_current;
    const sep: ContextMenuItem = { separator: true, label: "", action: () => undefined };

    const copyName: ContextMenuItem = { label: "Copy Name", action: () => void navigator.clipboard.writeText(branch.name) };

    if (branch.is_remote) {
      return [
        { label: "Checkout (create local)", shortcut: "\u23CE", action: () => void handleCheckout(branch) },
        sep,
        { label: "Fetch", shortcut: "f", action: () => void doFetch(branch) },
        { label: "Compare with current", action: () => void doCompare(branch), disabled: !cur },
        sep,
        { label: "Open on GitHub", action: () => void openBranchOnGitHub(branch) },
        copyName,
      ];
    }

    if (isCurrent) {
      return [
        { label: "Push", shortcut: "\u21E7P", action: () => void doPush(branch) },
        { label: "Pull", shortcut: "p", action: () => void doPull(branch) },
        { label: "Fetch", shortcut: "f", action: () => void doFetch(branch) },
        { label: "Update from base (rebase)", action: () => void doUpdateFromBase(branch) },
        sep,
        { label: "Rename", shortcut: "\u21E7R", action: () => startRename(branch, getFlatIndex(branch)) },
        sep,
        copyName,
      ];
    }

    const items: ContextMenuItem[] = [
      { label: "Checkout", shortcut: "\u23CE", action: () => void handleCheckout(branch) },
      { label: "Merge into current", shortcut: "\u21E7M", action: () => startMerge(branch), disabled: !cur },
      { label: "Rebase onto", shortcut: "r", action: () => startRebase(branch), disabled: !cur },
      { label: "Compare with current", action: () => void doCompare(branch), disabled: !cur },
      sep,
      { label: "Push", shortcut: "\u21E7P", action: () => void doPush(branch) },
      { label: "Pull", shortcut: "p", action: () => void doPull(branch) },
      { label: "Fetch", shortcut: "f", action: () => void doFetch(branch) },
      { label: "Update from base (rebase)", action: () => void doUpdateFromBase(branch) },
    ];

    if (!branch.is_main) {
      items.push(sep);
      items.push({ label: "Rename", shortcut: "\u21E7R", action: () => startRename(branch, getFlatIndex(branch)) });
      items.push({ label: "Delete", shortcut: "d", action: () => startDelete(branch) });
    }

    items.push(sep);
    items.push(copyName);

    return items;
  }

  // --- Keyboard navigation ---

  function handleKeyDown(e: KeyboardEvent) {
    // Don't intercept when rename input or create input is focused
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" && target !== containerRef) {
      // Handle rename input
      if (renamingIndex() >= 0) {
        if (e.key === "Enter") { e.preventDefault(); doRenameBranch(); }
        if (e.key === "Escape") { e.preventDefault(); cancelRename(); }
      }
      // Handle create input
      if (creating()) {
        if (e.key === "Enter") { e.preventDefault(); doCreateBranch(); }
        if (e.key === "Escape") { e.preventDefault(); cancelCreate(); }
      }
      return;
    }

    const flat = flatVisible();
    const idx = selectedIndex();

    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (flat.length === 0) return;
      setSelectedIndex(idx < flat.length - 1 ? idx + 1 : 0);
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      if (flat.length === 0) return;
      setSelectedIndex(idx <= 0 ? flat.length - 1 : idx - 1);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      if (search()) { setSearch(""); return; }
      setSelectedIndex(-1);
      return;
    }
    if (e.key === "/" ) {
      e.preventDefault();
      searchInputRef?.focus();
      return;
    }

    // Key actions require a selected branch
    if (idx < 0 || idx >= flat.length) {
      if (e.key === "n") { e.preventDefault(); startCreate(); }
      return;
    }
    const branch = flat[idx];

    if (e.key === "Enter") {
      e.preventDefault();
      handleCheckout(branch);
      return;
    }
    if (e.key === "n") {
      e.preventDefault();
      startCreate();
      return;
    }
    if (e.key === "d") {
      e.preventDefault();
      startDelete(branch);
      return;
    }
    if (e.key === "R") {
      e.preventDefault();
      startRename(branch, idx);
      return;
    }
    if (e.key === "M") {
      e.preventDefault();
      startMerge(branch);
      return;
    }
    if (e.key === "r") {
      e.preventDefault();
      startRebase(branch);
      return;
    }
    if (e.key === "P") {
      e.preventDefault();
      doPush(branch);
      return;
    }
    if (e.key === "p") {
      e.preventDefault();
      doPull(branch);
      return;
    }
    if (e.key === "f") {
      e.preventDefault();
      doFetch(branch);
      return;
    }
  }

  // Focus container on mount so keyboard nav works immediately
  createEffect(() => {
    if (!loading() && branches().length > 0) {
      containerRef?.focus();
    }
  });

  // Helper: get the global flat index for a branch
  function getFlatIndex(branch: BranchDetail): number {
    return flatIndexMap().get(`${branch.name}:${branch.is_remote}`) ?? -1;
  }

  // --- Dialog helpers ---

  const dialogData = createMemo(() => {
    const d = dialog();
    if (!d) return null;
    if (d.type === "delete") {
      const { branch } = d;
      const info = branch.is_merged
        ? "This branch is merged into main."
        : "This branch has unmerged commits.";
      return {
        title: `Delete branch "${branch.name}"?`,
        message: info,
        branch,
      };
    }
    if (d.type === "merge") {
      const { branch, currentBranch: cur } = d;
      return {
        title: `Merge into ${cur}`,
        message: `Merge "${branch.name}" into "${cur}"?`,
        branch,
      };
    }
    if (d.type === "rebase") {
      const { branch, currentBranch: cur } = d;
      return {
        title: `Rebase ${cur}`,
        message: `Rebase current branch "${cur}" onto "${branch.name}"?`,
        branch,
      };
    }
    return null;
  });

  // --- Render helpers ---

  function renderBranchName(branch: BranchDetail, flatIndex: number) {
    const isRenaming = renamingIndex() === flatIndex;
    if (isRenaming) {
      return (
        <input
          ref={renameInputRef}
          class={s.inlineInput}
          value={renameValue()}
          onInput={(e) => setRenameValue(e.currentTarget.value)}
          onClick={(e) => e.stopPropagation()}
          onDblClick={(e) => e.stopPropagation()}
          // keydown handled at container level
        />
      );
    }
    return (
      <span
        class={cx(
          s.branchName,
          branch.is_current && s.branchNameBold,
          branch.is_current && s.branchCurrent,
        )}
      >
        {branch.name}
      </span>
    );
  }

  function renderBranchRow(branch: BranchDetail, isLocal: boolean, indented = false) {
    const flatIndex = () => getFlatIndex(branch);
    const isSelected = () => selectedIndex() === flatIndex();
    return (
      <div
        class={cx(
          s.branchRow,
          indented && s.branchRowIndented,
          isStale(branch.last_commit_date) && s.stale,
          isSelected() && s.selected,
        )}
        title={branchTooltip(branch)}
        onClick={() => { setSelectedIndex(flatIndex()); containerRef?.focus(); }}
        onDblClick={() => handleCheckout(branch)}
        onContextMenu={(e) => openContextMenu(e, branch)}
      >
        <Show
          when={isLocal && branch.is_current}
          fallback={<span class={s.branchIconPlaceholder} />}
        >
          <span class={s.branchCurrentIcon}>
            <CheckIcon />
          </span>
        </Show>
        <Show
          when={isLocal}
          fallback={<span class={s.branchName}>{branch.name}</span>}
        >
          {renderBranchName(branch, flatIndex())}
        </Show>
        <span class={s.branchMeta}>
          <Show when={isLocal && (branch.ahead ?? 0) > 0}>
            <span class={s.ahead}>&#x2191;{branch.ahead}</span>
          </Show>
          <Show when={isLocal && (branch.behind ?? 0) > 0}>
            <span class={s.behind}>&#x2193;{branch.behind}</span>
          </Show>
          <Show when={isLocal && (branch.base_behind ?? 0) > 0}>
            <span class={s.baseBehind} title={`${branch.base_behind} behind ${branch.base_branch ?? "base"}`}>&#x21E3;{branch.base_behind}</span>
          </Show>
          <Show when={branch.is_merged}>
            <span class={s.merged}>merged</span>
          </Show>
          <Show when={branch.last_commit_date}>
            <span class={s.metaDate}>{relativeDate(branch.last_commit_date)}</span>
          </Show>
        </span>
      </div>
    );
  }

  /** Render a grouped or flat list of branch rows for a section */
  function renderBranchList(
    branchList: BranchDetail[],
    section: "local" | "remote",
    renderRow: (b: BranchDetail, indented?: boolean) => ReturnType<Component>,
  ) {
    if (!foldingEnabled()) {
      return (
        <For each={branchList}>
          {(b) => renderRow(b)}
        </For>
      );
    }

    const { ungrouped, groups } = groupBranchesByPrefix(branchList);
    return (
      <>
        <For each={ungrouped}>{(b) => renderRow(b)}</For>
        <For each={groups}>
          {(group) => {
            const key = `${section}:${group.prefix}`;
            const isCollapsed = () => collapsedGroups().has(key);
            return (
              <>
                <div class={s.groupHeader} onClick={() => toggleGroup(key)}>
                  <span class={cx(s.chevron, isCollapsed() && s.chevronCollapsed)}>&#x25BC;</span>
                  <span class={s.groupIcon}><FolderIcon /></span>
                  <span class={s.groupPrefix}>{group.prefix}</span>
                  <span class={s.sectionCount}>{group.branches.length}</span>
                </div>
                <Show when={!isCollapsed()}>
                  <div class={s.groupChildren}>
                    <For each={group.branches}>{(b) => renderRow(b, true)}</For>
                  </div>
                </Show>
              </>
            );
          }}
        </For>
      </>
    );
  }

  // Cleanup on unmount
  onCleanup(() => {
    setDialog(null);
    setDirtyCheckout(null);
  });

  return (
    <div
      ref={containerRef}
      class={s.container}
      tabIndex={-1}
      onKeyDown={handleKeyDown}
    >
      <div class={s.searchBar}>
        <input
          ref={searchInputRef}
          class={s.searchInput}
          type="text"
          placeholder="Filter branches... (/ to focus)"
          value={search()}
          onInput={(e) => setSearch(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              setSearch("");
              containerRef?.focus();
            }
          }}
        />
        <button
          class={cx(s.foldToggle, foldingEnabled() && s.foldToggleActive)}
          title={foldingEnabled() ? "Disable prefix grouping" : "Enable prefix grouping"}
          onClick={() => setFoldingEnabled((v) => !v)}
        >
          <FolderIcon />
        </button>
      </div>

      {/* Create branch inline form */}
      <Show when={creating()}>
        <div class={s.createForm}>
          <input
            ref={createInputRef}
            class={s.inlineInput}
            placeholder="New branch name..."
            value={createState().name}
            onInput={(e) => setCreateState((p) => ({ ...p, name: e.currentTarget.value }))}
          />
          <Show when={createBaseRefs().length > 0}>
            <select
              class={s.fromSelect}
              value={createState().startPoint ?? ""}
              onChange={(e) => setCreateState((p) => ({ ...p, startPoint: e.currentTarget.value || null }))}
            >
              <option value="">HEAD</option>
              <optgroup label="Local">
                <For each={createBaseRefs().filter(r => r.kind === "local")}>
                  {(ref) => <option value={ref.name}>{ref.name}{ref.is_default ? " (default)" : ""}</option>}
                </For>
              </optgroup>
              <Show when={createBaseRefs().some(r => r.kind === "remote")}>
                <optgroup label="Remote">
                  <For each={createBaseRefs().filter(r => r.kind === "remote")}>
                    {(ref) => <option value={ref.name}>{ref.name}</option>}
                  </For>
                </optgroup>
              </Show>
            </select>
          </Show>
          <label class={s.createCheckboxLabel}>
            <input
              type="checkbox"
              checked={createState().checkout}
              onChange={(e) => setCreateState((p) => ({ ...p, checkout: e.currentTarget.checked }))}
            />
            Checkout
          </label>
          <button class={s.createConfirmBtn} onClick={doCreateBranch}>Create</button>
          <button class={s.createCancelBtn} onClick={cancelCreate}>Cancel</button>
        </div>
      </Show>

      <Show when={props.repoPath}>
        <div class={s.smartActions}>
          <SmartButtonStrip
            placement="git-branches"
            repoPath={props.repoPath!}
            defaultPromptId="smart-create-pr"
          />
        </div>
      </Show>

      <Show when={!loading()} fallback={<div class={s.empty}>Loading branches...</div>}>
        <Show when={branches().length > 0} fallback={<div class={s.empty}>No branches</div>}>

          {/* Recent section */}
          <Show when={recentBranches().length > 0}>
            <div
              class={s.sectionHeader}
              onClick={() => setRecentExpanded((v) => !v)}
            >
              <span class={cx(s.chevron, !recentExpanded() && s.chevronCollapsed)}>&#x25BC;</span>
              Recent
              <span class={s.sectionCount}>{recentBranches().length}</span>
            </div>
            <Show when={recentExpanded()}>
              <For each={recentBranches()}>
                {(branch) => renderBranchRow(branch, true)}
              </For>
            </Show>
          </Show>

          {/* Local section */}
          <div
            class={s.sectionHeader}
            onClick={() => setLocalExpanded((v) => !v)}
          >
            <span class={cx(s.chevron, !localExpanded() && s.chevronCollapsed)}>&#x25BC;</span>
            Local
            <span class={s.sectionCount}>{localBranches().length}</span>
          </div>
          <Show when={localExpanded()}>
            {renderBranchList(filteredLocal(), "local", (b, indented) => renderBranchRow(b, true, indented))}
          </Show>

          {/* Remote section - only show when there are remote branches */}
          <Show when={remoteBranches().length > 0}>
            <div
              class={s.sectionHeader}
              onClick={() => setRemoteExpanded((v) => !v)}
            >
              <span class={cx(s.chevron, !remoteExpanded() && s.chevronCollapsed)}>&#x25BC;</span>
              Remote
              <span class={s.sectionCount}>{remoteBranches().length}</span>
            </div>
            <Show when={remoteExpanded()}>
              {renderBranchList(filteredRemote(), "remote", (b, indented) => renderBranchRow(b, false, indented))}
            </Show>
          </Show>

        </Show>
      </Show>


      {/* Dirty checkout dialog */}
      <Show when={dirtyCheckout() !== null}>
        <div class={s.overlay}>
          <div class={s.popover}>
            <div class={s.popoverHeader}>
              <h4>Uncommitted changes</h4>
            </div>
            <div class={s.popoverBody}>
              <p class={s.confirmInfo}>
                The working tree has uncommitted changes.
                How do you want to switch to <strong>{dirtyCheckout()?.branchName}</strong>?
              </p>
            </div>
            <div class={s.popoverActions}>
              <button class={s.cancelBtn} onClick={() => setDirtyCheckout(null)}>Cancel</button>
              <button class={s.secondaryBtn} onClick={handleDirtyCheckoutForce}>Force Switch</button>
              <button class={s.primaryBtn} onClick={handleDirtyCheckoutStash}>Stash and Switch</button>
            </div>
          </div>
        </div>
      </Show>

      {/* Delete confirm dialog */}
      <Show when={dialog()?.type === "delete"}>
        <div class={s.overlay}>
          <div class={s.popover}>
            <div class={s.popoverHeader}>
              <h4>{dialogData()?.title}</h4>
            </div>
            <div class={s.popoverBody}>
              <p class={s.confirmInfo}>{dialogData()?.message}</p>
            </div>
            <div class={s.popoverActions}>
              <button class={s.cancelBtn} onClick={() => setDialog(null)}>Cancel</button>
              <Show when={!dialogData()?.branch.is_merged}>
                <button class={s.dangerBtn} onClick={() => doDeleteBranch(true)}>Force Delete</button>
              </Show>
              <button class={s.primaryBtn} onClick={() => doDeleteBranch(false)}>Delete</button>
            </div>
          </div>
        </div>
      </Show>

      {/* Merge confirm dialog */}
      <ConfirmDialog
        visible={dialog()?.type === "merge"}
        title={dialogData()?.title ?? ""}
        message={dialogData()?.message ?? ""}
        confirmLabel="Merge"
        kind="warning"
        onClose={() => setDialog(null)}
        onConfirm={doMerge}
      />

      {/* Rebase confirm dialog */}
      <ConfirmDialog
        visible={dialog()?.type === "rebase"}
        title={dialogData()?.title ?? ""}
        message={dialogData()?.message ?? ""}
        confirmLabel="Rebase"
        kind="warning"
        onClose={() => setDialog(null)}
        onConfirm={doRebase}
      />

      {/* Branch context menu */}
      <ContextMenu
        items={ctxBranch() ? buildContextMenuItems(ctxBranch()!) : []}
        x={ctxMenu.position().x}
        y={ctxMenu.position().y}
        visible={ctxMenu.visible()}
        onClose={ctxMenu.close}
      />
    </div>
  );
};

export default BranchesTab;
