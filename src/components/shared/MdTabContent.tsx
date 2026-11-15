import { type Component, lazy, Suspense } from "solid-js";
import {
	type ClaudeUsageTab as ClaudeUsageTabData,
	type CodexUsageTab as CodexUsageTabData,
	type GithubOpsTab as GithubOpsTabData,
	type GrokUsageTab as GrokUsageTabData,
	type MdTabData,
	mdTabsStore,
	type PrDiffTab as PrDiffTabData,
} from "../../stores/mdTabs";

const ClaudeUsageDashboard = lazy(() =>
	import("../ClaudeUsageDashboard").then((module) => ({ default: module.ClaudeUsageDashboard })),
);
const CodexUsageDashboard = lazy(() =>
	import("../CodexUsageDashboard").then((module) => ({ default: module.CodexUsageDashboard })),
);
const GrokUsageDashboard = lazy(() =>
	import("../GrokUsageDashboard").then((module) => ({ default: module.GrokUsageDashboard })),
);
const CommandOverview = lazy(() =>
	import("../CommandOverview").then((module) => ({ default: module.CommandOverview })),
);
const GithubOpsDashboard = lazy(() =>
	import("../GithubOpsDashboard").then((module) => ({ default: module.GithubOpsDashboard })),
);
const TasksShell = lazy(() => import("../Tasks/TasksShell").then((module) => ({ default: module.TasksShell })));
const HtmlPreviewTab = lazy(() => import("../HtmlPreviewTab").then((module) => ({ default: module.HtmlPreviewTab })));
const MarkdownTab = lazy(() => import("../MarkdownTab").then((module) => ({ default: module.MarkdownTab })));
const PluginPanel = lazy(() => import("../PluginPanel").then((module) => ({ default: module.PluginPanel })));
const PrDiffTab = lazy(() => import("../PrDiffTab").then((module) => ({ default: module.PrDiffTab })));

/** Renders the correct component for a given MdTab type. Shared between TerminalArea and PaneTree. */
export const MdTabContent: Component<{ tab: MdTabData; onClose: () => void; visible?: () => boolean }> = (props) => {
	const tab = props.tab;
	return (
		<Suspense>
			{(() => {
				if (tab.type === "claude-usage")
					return <ClaudeUsageDashboard sessionId={() => (props.tab as ClaudeUsageTabData).sessionId} />;
				if (tab.type === "codex-usage")
					return <CodexUsageDashboard sessionId={() => (props.tab as CodexUsageTabData).sessionId} />;
				if (tab.type === "grok-usage")
					return <GrokUsageDashboard sessionId={() => (props.tab as GrokUsageTabData).sessionId} />;
				if (tab.type === "github-ops") return <GithubOpsDashboard repoPath={(tab as GithubOpsTabData).repoPath} />;
				if (tab.type === "command-overview") return <CommandOverview />;
				if (tab.type === "tasks") return <TasksShell visible={props.visible} />;
				if (tab.type === "plugin-panel")
					return <PluginPanel tab={tab} onClose={props.onClose} visible={props.visible} />;
				if (tab.type === "pr-diff") {
					const pr = tab as PrDiffTabData;
					return (
						<PrDiffTab
							prNumber={pr.prNumber}
							prTitle={pr.prTitle}
							diff={pr.diff}
							headSha={pr.headSha}
							onPin={(sha) => mdTabsStore.pinPrDiffHead(pr.id, sha)}
							repoPath={pr.repoPath}
						/>
					);
				}
				if (tab.type === "html-preview") return <HtmlPreviewTab tab={tab} onClose={props.onClose} />;
				return <MarkdownTab tab={tab} onClose={props.onClose} />;
			})()}
		</Suspense>
	);
};
