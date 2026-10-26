import { afterEach, expect, it, vi } from "vitest";
import { startBackendSkewWarning } from "../backendSkew";

afterEach(() => {
	document.body.replaceChildren();
	vi.unstubAllGlobals();
});

it("does not nag after frontend-only landings and clears resolved staleness on HMR", async () => {
	const reply = vi.fn().mockResolvedValue(new Response(JSON.stringify({ status: "current", key: "same" })));
	vi.stubGlobal("fetch", reply);
	let update: (() => void) | undefined;
	const stop = startBackendSkewWarning(
		{ exe: "/cargo/debug/app", started_at_ms: 123, git_hash: "backend-commit" },
		{
			on: (_event, callback) => {
				update = callback;
			},
			off: vi.fn(),
		},
	);
	await vi.waitFor(() => expect(reply).toHaveBeenCalledOnce());
	// Catches: the Vite check loses the running executable/start-time binding.
	const launch = JSON.parse(new URL(reply.mock.calls[0][0], "http://localhost").searchParams.get("backend") ?? "null");
	expect(launch).toMatchObject({ exe: "/cargo/debug/app", started_at_ms: 123 });
	expect(document.querySelector('[role="alert"]')).toBeNull();
	reply.mockResolvedValue(
		new Response(JSON.stringify({ status: "stale", key: "changed", backend_id: "old", tree_id: "new" })),
	);
	update?.();
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull());
	reply.mockResolvedValue(new Response(JSON.stringify({ status: "current", key: "rebuilt" })));
	update?.();
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).toBeNull());
	stop();
});

it("keeps a dismissed mismatch quiet across HMR but warns for a new backend tree", async () => {
	const reply = vi
		.fn()
		.mockImplementation(() =>
			Promise.resolve(
				new Response(JSON.stringify({ status: "stale", key: "changed", backend_id: "old", tree_id: "new" })),
			),
		);
	vi.stubGlobal("fetch", reply);
	let update: (() => void) | undefined;
	const stop = startBackendSkewWarning(
		{ exe: "/cargo/debug/app", started_at_ms: 123, git_hash: "sha" },
		{
			on: (_event, callback) => {
				update = callback;
			},
			off: vi.fn(),
		},
	);
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull());
	(document.querySelector('[aria-label="Dismiss backend restart warning"]') as HTMLButtonElement).click();
	update?.();
	await vi.waitFor(() => expect(reply).toHaveBeenCalledTimes(2));
	expect(document.querySelector('[role="alert"]')).toBeNull();
	reply.mockResolvedValue(
		new Response(JSON.stringify({ status: "stale", key: "another", backend_id: "old", tree_id: "newer" })),
	);
	update?.();
	await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull());
	stop();
});

it("warns for a pre-feature backend instead of silently treating missing identity as a match", async () => {
	vi.stubGlobal(
		"fetch",
		vi.fn().mockResolvedValue(new Response(JSON.stringify({ status: "unavailable", key: "unavailable" }))),
	);
	// Real Vite exposes off(); Vitest's default hot stub does not.
	const stop = startBackendSkewWarning(undefined, { on: vi.fn(), off: vi.fn() });
	await vi.waitFor(() =>
		expect(document.querySelector('[role="alert"]')?.textContent).toContain("Backend identity unavailable"),
	);
	stop();
});
