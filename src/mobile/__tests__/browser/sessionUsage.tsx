import { render } from "solid-js/web";
import { SessionDetailScreen } from "../../screens/SessionDetailScreen";
import "../../mobile.css";

// Screenshot fixture only. The HTTP/SSE boundary is covered by Rust and Vitest.
window.fetch = async () => new Response(JSON.stringify({ lines: [], screen: [], total_lines: 0 }));
document.getElementById("root")!.style.cssText = "height:100dvh;display:flex;flex-direction:column";
render(
	() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={{
				session_id: "usage-visual-fixture",
				cwd: "/repo/tuicommander",
				worktree_path: null,
				worktree_branch: null,
				state: { agent_type: "ego", awaiting_input: false, rate_limited: false, last_activity_ms: Date.now() },
			}}
			sessionExists
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	),
	document.getElementById("root")!,
);

// Open only this fixture's menu; no live session or desktop access.
requestAnimationFrame(() => {
	(document.querySelector('[aria-label="More session actions"]') as HTMLButtonElement)?.click();
});

// DEFERRED (2026-10-23) — headless screenshot capture timed out; rerun this isolated fixture when desktop capture is available.
