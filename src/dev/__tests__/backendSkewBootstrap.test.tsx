import { expect, it, vi } from "vitest";

vi.mock("../../App", () => ({ default: () => <main>Application</main> }));
vi.mock("../../debugGlobals", () => ({ initDebugGlobals: vi.fn() }));
vi.mock("../../utils/frontendHeartbeat", () => ({ startFrontendHeartbeat: vi.fn() }));
vi.mock("../../stores/dragDrop", () => ({ initDragDrop: vi.fn() }));
vi.mock("../../utils/externalLinkClick", () => ({ handleExternalLinkClick: vi.fn() }));
vi.mock("../simulator", () => ({}));

// Catches: the dev bootstrap does not start the warning, leaving new UI commands
// silently connected to an old backend even if the warning component works.
it("shows backend restart warning on application load when served backend inputs differ", async () => {
	document.body.innerHTML = '<div id="app"></div>';
	Object.assign(window, {
		__TAURI_INTERNALS__: {},
		__TUIC_DEV_BACKEND__: { exe: "/cargo/debug/app", started_at_ms: 123, git_hash: "compiled-sha" },
	});
	vi.stubGlobal(
		"fetch",
		vi.fn().mockResolvedValue(
			new Response(
				JSON.stringify({
					status: "stale",
					key: "newer-input",
					backend_id: "compiled-sha/entry",
					tree_id: "served-sha/input",
				}),
			),
		),
	);
	await import("../../appEntry");
	await vi.waitFor(() =>
		expect(document.querySelector('[role="alert"]')?.textContent ?? "").toContain("Restart tauri dev"),
	);
	expect(document.body.textContent).toContain("compiled-sha");
	expect(document.body.textContent).toContain("served-sha");
	vi.unstubAllGlobals();
});
