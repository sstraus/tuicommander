import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import "./mocks/tauri";
import { applySessionStateEvent } from "../hooks/useAgentPolling";
import { SessionCard } from "../mobile/components/SessionCard";
import type { SessionState } from "../mobile/useSessions";
import { terminalsStore as store } from "../stores/terminals";
import { makeTerminal } from "./helpers/store";

afterEach(cleanup);

// Story 2636-5551: desktop and mobile read the same SessionState snapshot.
describe("tool readout parity", () => {
	const snapshot = (tool?: string): SessionState => ({
		awaiting_input: false,
		rate_limited: false,
		shell_state: "busy",
		agent_type: "claude",
		last_activity_ms: 1,
		...(tool ? { current_tool: tool } : {}),
	});

	const mobileTool = (state: SessionState) => {
		const { container, unmount } = render(() => (
			<SessionCard
				session={{ session_id: "s", cwd: "/p", worktree_path: null, worktree_branch: null, state }}
				onSelect={() => {}}
			/>
		));
		const text = container.querySelector("[data-testid='tool-row']")?.textContent ?? null;
		unmount();
		return text;
	};

	// Catches: one client drops or keeps a tool the other shows, or an omitted
	// field (Rust skips None) is read as "unchanged" and leaves a stale tool.
	it("shows the same tool and retracts it on an omitted field", () => {
		const id = store.add(makeTerminal({ sessionId: "tool-2636" }));
		for (const tool of ["Bash", undefined, "run_terminal_command", undefined]) {
			const state = snapshot(tool);
			applySessionStateEvent({ session_id: "tool-2636", state });
			expect(store.get(id)?.currentTool ?? null).toBe(tool ?? null);
			expect(mobileTool(state)).toBe(tool ?? null);
		}
		store.remove(id);
	});
});
