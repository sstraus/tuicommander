import { Component } from "solid-js";
import type { MdTabData, PrDiffTab as PrDiffTabData } from "../../stores/mdTabs";
import { ClaudeUsageDashboard } from "../ClaudeUsageDashboard";
import { CommandOverview } from "../CommandOverview";
import { PluginPanel } from "../PluginPanel";
import { PrDiffTab } from "../PrDiffTab";
import { HtmlPreviewTab } from "../HtmlPreviewTab";
import { MarkdownTab } from "../MarkdownTab";

/** Renders the correct component for a given MdTab type. Shared between TerminalArea and PaneTree. */
export const MdTabContent: Component<{ tab: MdTabData; onClose: () => void }> = (props) => {
  const tab = props.tab;
  if (tab.type === "claude-usage") return <ClaudeUsageDashboard />;
  if (tab.type === "command-overview") return <CommandOverview />;
  if (tab.type === "plugin-panel") return <PluginPanel tab={tab} onClose={props.onClose} />;
  if (tab.type === "pr-diff") {
    const pr = tab as PrDiffTabData;
    return <PrDiffTab prNumber={pr.prNumber} prTitle={pr.prTitle} diff={pr.diff} />;
  }
  if (tab.type === "html-preview") return <HtmlPreviewTab tab={tab} onClose={props.onClose} />;
  return <MarkdownTab tab={tab} onClose={props.onClose} />;
};
