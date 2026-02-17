// Re-export all stores for convenient imports

export { terminalsStore } from "./terminals";
export type { TerminalData, TerminalRef, TerminalState } from "./terminals";

export { repositoriesStore } from "./repositories";

export { uiStore } from "./ui";

export { promptStore } from "./prompt";

export { settingsStore, IDE_NAMES, FONT_FAMILIES } from "./settings";
export type { IdeType, FontType } from "./settings";

export { rateLimitStore } from "./ratelimit";

export { promptLibraryStore } from "./promptLibrary";
export type { SavedPrompt, PromptCategory, PromptVariable } from "./promptLibrary";

export { agentFallbackStore } from "./agentFallback";
export type { FallbackChainConfig } from "./agentFallback";

export { tasksStore } from "./tasks";
export type { TaskData, TaskStatus, TaskCompletionCallback } from "./tasks";

export { errorHandlingStore } from "./errorHandling";
export type { RetryInfo } from "./errorHandling";

export { notificationsStore } from "./notifications";

export { repoSettingsStore } from "./repoSettings";
export type { RepoSettings } from "./repoSettings";

export { githubStore } from "./github";
