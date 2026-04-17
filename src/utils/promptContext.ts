import { promptLibraryStore, type SavedPrompt } from "../stores/promptLibrary";
import { toastsStore } from "../stores/toasts";
import { appLogger } from "../stores/appLogger";
import type { ContextMenuItem } from "../components/ContextMenu";
import type { useSmartPrompts } from "../hooks/useSmartPrompts";
import { t } from "../i18n";

/**
 * Shared helpers for `placement: "file-context"` smart prompts.
 * Used by every host that surfaces a file/folder: file browser,
 * diff editor, markdown tabs, editor tabs.
 */
export interface FileContextInput {
  /** Absolute filesystem path. */
  absPath: string;
  /** Repo root if known — used to compute `file_rel_path`. */
  repoRoot?: string | null;
  /** Whether the path is a directory (folder selected in browser). */
  isDir?: boolean;
}

/** Build the variable map passed to `executeSmartPrompt` as `manualVariables`. */
export function fileContextVariables(input: FileContextInput): Record<string, string> {
  const abs = input.absPath;
  const lastSlash = abs.lastIndexOf("/");
  const name = lastSlash >= 0 ? abs.slice(lastSlash + 1) : abs;
  const dir = lastSlash >= 0 ? abs.slice(0, lastSlash) : "";

  const dotIdx = name.lastIndexOf(".");
  const ext = dotIdx > 0 ? name.slice(dotIdx) : "";

  let relPath = abs;
  if (input.repoRoot) {
    const root = input.repoRoot.endsWith("/") ? input.repoRoot : `${input.repoRoot}/`;
    if (abs.startsWith(root)) relPath = abs.slice(root.length);
  }

  return {
    file_path: abs,
    file_rel_path: relPath,
    file_name: name,
    file_ext: ext,
    file_dir: dir,
    file_is_dir: input.isDir ? "true" : "false",
  };
}

/**
 * Build a "Smart Prompts ▶" submenu from all prompts registered with
 * placement="file-context". Returns null when there are no prompts so
 * callers can skip the entry entirely.
 */
export function fileContextSmartMenuItem(
  input: FileContextInput,
  smartPrompts: ReturnType<typeof useSmartPrompts>,
  opts?: { separator?: boolean },
): ContextMenuItem | null {
  const prompts = promptLibraryStore.getSmartByPlacement("file-context");
  if (prompts.length === 0) return null;

  const vars = fileContextVariables(input);

  const children: ContextMenuItem[] = prompts.map((prompt: SavedPrompt) => ({
    label: prompt.name,
    action: () => { void runFileContextPrompt(prompt, vars, smartPrompts); },
  }));

  return {
    label: t("smartPrompts.fileContextMenu", "Smart Prompts"),
    action: () => { /* parent — no-op */ },
    children,
    separator: opts?.separator ?? false,
  };
}

async function runFileContextPrompt(
  prompt: SavedPrompt,
  vars: Record<string, string>,
  smartPrompts: ReturnType<typeof useSmartPrompts>,
): Promise<void> {
  promptLibraryStore.markAsUsed(prompt.id);
  try {
    const result = await smartPrompts.executeSmartPrompt(prompt, vars);
    if (!result.ok) {
      toastsStore.add(prompt.name, result.reason ?? "Failed", "warn");
    }
  } catch (err) {
    appLogger.error("prompts", `Failed to execute "${prompt.name}"`, err);
    toastsStore.add(prompt.name, String(err), "error");
  }
}
