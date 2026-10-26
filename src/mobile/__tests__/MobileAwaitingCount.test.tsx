// @vitest-environment jsdom
import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import MobileApp from "../MobileApp";

vi.mock("../../transport", () => ({
	isTauri: () => false,
	rpc: vi.fn(async (command: string) =>
		command === "list_active_sessions"
			? [
					...Array.from({ length: 3 }, (_, index) => ({
						session_id: `waiting-${index}`,
						display_name: `Waiting agent ${index}`,
						cwd: "/work/project",
						state: { awaiting_input: true, agent_type: "claude", question_text: `Question ${index}?` },
					})),
					{ session_id: "idle", display_name: "Idle agent", cwd: "/work/project", state: { awaiting_input: false } },
				]
			: {},
	),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../mobileTheme", () => ({ loadMobileTheme: vi.fn(async () => "commander") }));
vi.mock("../useMobileNotifications", () => ({ useMobileNotifications: vi.fn() }));
vi.mock("../useVersionCheck", () => ({
	useVersionCheck: () => ({ updateAvailable: () => false, applyUpdate: vi.fn() }),
}));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

afterEach(cleanup);

// Catches: the header badge duplicates the AWAITING tile's total on the real mounted home.
it("renders exactly one awaiting count for three waiting sessions on mobile home", async () => {
	history.replaceState(null, "", "/mobile");
	const view = render(() => <MobileApp />);
	await waitFor(() => expect(view.getByText("Awaiting")).toBeTruthy());
	expect(view.getAllByText("3", { exact: true })).toHaveLength(1);
	expect(view.getByText("Working").parentElement?.textContent).toBe("0Working");
	expect(view.getAllByText("Idle")[0].parentElement?.textContent).toBe("1Idle");
	for (const name of ["Waiting agent 0", "Waiting agent 1", "Waiting agent 2"]) {
		expect(view.getByRole("button", { name: new RegExp(`${name}.*Question`) })).toBeTruthy();
	}
});
