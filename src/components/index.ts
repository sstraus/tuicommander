// Re-export all components

// UI components
export * from "./ui";

// Terminal component
export { Terminal } from "./Terminal";
export type { TerminalProps } from "./Terminal";

// Container components
export { Sidebar } from "./Sidebar";
export type { SidebarProps } from "./Sidebar";

export { Toolbar } from "./Toolbar";
export type { ToolbarProps } from "./Toolbar";

export { TabBar } from "./TabBar";
export type { TabBarProps } from "./TabBar";

export { StatusBar } from "./StatusBar";
export type { StatusBarProps } from "./StatusBar";

// Panel components
export { DiffPanel } from "./DiffPanel";
export type { DiffPanelProps } from "./DiffPanel";

export { MarkdownPanel } from "./MarkdownPanel";
export type { MarkdownPanelProps } from "./MarkdownPanel";

export { PromptOverlay } from "./PromptOverlay";
export type { PromptOverlayProps } from "./PromptOverlay";

export { PromptDrawer } from "./PromptDrawer";
export type { PromptDrawerProps } from "./PromptDrawer";

export { SettingsPanel } from "./SettingsPanel";
export type { SettingsPanelProps } from "./SettingsPanel";

export { BranchPopover } from "./BranchPopover";
export type { BranchPopoverProps } from "./BranchPopover";

export { IdeLauncher } from "./IdeLauncher";
export type { IdeLauncherProps } from "./IdeLauncher";

export { TaskQueuePanel } from "./TaskQueuePanel";
export type { TaskQueuePanelProps } from "./TaskQueuePanel";

export { ContextMenu, createContextMenu } from "./ContextMenu";
export type { ContextMenuProps, ContextMenuItem } from "./ContextMenu";

export { GitOperationsPanel } from "./GitOperationsPanel";
export type { GitOperationsPanelProps } from "./GitOperationsPanel";

export { RunCommandDialog } from "./RunCommandDialog";
export type { RunCommandDialogProps } from "./RunCommandDialog";
