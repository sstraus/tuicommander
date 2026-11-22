import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
// @ts-expect-error — untyped plugin JS module in the plugins submodule.
import plugin, { renderDashboard } from "../../../plugins/mdkb-dashboard/main.js";

function dashboard(hooks: Record<string, unknown>) {
	const page = new DOMParser().parseFromString(
		renderDashboard({ stats: { hooks }, memories: [], config: null, error: null }, "/repo"),
		"text/html",
	);
	return page.body;
}

// Domain inputs follow mdkb HooksSummary in src/cli/stats_report.rs:236.
// This is a renderer contract, not a recorded external CLI fixture.
const event = { event: "session_start", invocations: 12, fired: 8, avg_ms: 3, p95_ms: 7 };

describe("mdkb dashboard hook telemetry", () => {
	// Catches: disabled telemetry with no log rows silently hides the Hooks section.
	it("disabled hook telemetry is explained instead of hidden as zero activity", () => {
		const body = dashboard({ telemetry_enabled: false, events: [] });
		expect(body.textContent).toContain("Hook telemetry is off");
		expect(body.textContent).not.toContain("Calls");
	});

	// Catches: retained log rows are presented as current activity after telemetry is disabled.
	it("disabled hook telemetry suppresses retained activity metrics", () => {
		const body = dashboard({ telemetry_enabled: false, events: [event], slow_events_7d: 4 });
		expect(body.textContent).toContain("Hook telemetry is off");
		expect(body.querySelector("table")).toBeNull();
		expect(body.textContent).not.toContain("slow (7d)");
	});

	// Catches: enabled telemetry with no recorded events silently hides the Hooks section.
	it("enabled hook telemetry with no events shows zero activity", () => {
		const body = dashboard({ telemetry_enabled: true, events: [], slow_events_7d: 0 });
		expect([...body.querySelectorAll("h2")].map((heading) => heading.textContent)).toContain("Hooks");
		expect(body.textContent).toContain("No hook activity recorded");
		expect(body.textContent).not.toContain("Hook telemetry is off");
	});

	// Catches: a truthiness check treats older MDKB reports as disabled telemetry.
	it.each([true, undefined])("enabled or legacy telemetry (%s) retains event metrics", (enabled) => {
		const body = dashboard({ ...(enabled === undefined ? {} : { telemetry_enabled: enabled }), events: [event] });
		expect(body.textContent).not.toContain("Hook telemetry is off");
		expect(body.querySelector("tbody")?.textContent).toContain("session_start");
		expect([...body.querySelectorAll("tbody td")].map((cell) => cell.textContent)).toEqual([
			"session_start",
			"12",
			"8",
			"3",
			"7",
		]);
	});
});

// Embedding contract: mdkb src/store/vectors.rs IdentityStatus (stats.embedding, stats.code_embedding)
// and src/main.rs format_embed_result (generated, skipped, errors[]). Domain inputs, not recorded output.
const identity = {
	selected_model: "granite",
	current_model: "granite",
	pending_model: null,
	stale: false,
	ambiguous: false,
};

function embeddingRows(stats: Record<string, unknown>) {
	const page = new DOMParser().parseFromString(
		renderDashboard({ stats, memories: [], config: null, error: null }, "/repo"),
		"text/html",
	);
	const heading = [...page.querySelectorAll("h2")].find((h) => h.textContent === "Embeddings");
	return [...(heading?.closest(".dash-section")?.querySelectorAll("tbody tr") ?? [])].map((tr) => tr.textContent ?? "");
}

describe("mdkb dashboard embedding identities", () => {
	// Catches: a stale or pending code archive hides behind a current SQLite identity.
	it("shows document and code identities independently", () => {
		const rows = embeddingRows({
			embedding: identity,
			code_embedding: { ...identity, current_model: "minilm", stale: true },
		});
		expect(rows[0]).toContain("Current");
		expect(rows[1]).toContain("Re-embedding required");
	});

	// Catches: an interrupted regeneration (pending_model set) reads as Current or merely stale.
	it("pending model is reported as incomplete regeneration", () => {
		const rows = embeddingRows({ embedding: { ...identity, current_model: null, pending_model: "granite" } });
		expect(rows[0]).toContain("Re-embedding incomplete");
		expect(rows[0]).toContain("Pending: granite");
	});

	// Catches: ambiguous identity (mixed legacy rows) is rendered as a healthy store.
	it("ambiguous identity requires re-embedding", () => {
		expect(embeddingRows({ embedding: { ...identity, ambiguous: true } })[0]).toContain("Re-embedding required");
	});

	// Catches: an older mdkb without the embedding keys renders an empty Embeddings section.
	it("omits the section for stats without embedding identities", () => {
		expect(embeddingRows({})).toEqual([]);
	});
});

function activate(execCli: (bin: string, argv: string[], cwd: string) => Promise<string>) {
	let onMessage: ((m: unknown) => Promise<void> | void) | undefined;
	const panel = { update: vi.fn(() => true) };
	const host = {
		registerTerminalAction: vi.fn(),
		registerDashboard: vi.fn(),
		onStateChange: vi.fn(),
		setTicker: vi.fn(),
		clearTicker: vi.fn(),
		log: vi.fn(),
		getActiveRepo: () => ({ path: "/repo", displayName: "repo" }),
		execCli: vi.fn(execCli),
		openPanel: vi.fn((o: { onMessage: typeof onMessage }) => {
			onMessage = o.onMessage;
			return panel;
		}),
	};
	(plugin as { onload(h: unknown): void }).onload(host);
	return { host, panel, onMessage: () => onMessage! };
}

const statsJson = JSON.stringify({ index: { document_count: 1 }, embedding: identity });

async function runEmbed(embedOut: string) {
	const { host, panel, onMessage } = activate(async (_b, argv) => (argv.includes("embed") ? embedOut : statsJson));
	await host.registerDashboard.mock.calls[0][0].open();
	await onMessage()({ action: "run", cmd: "embed" });
	const html = String((panel.update.mock.calls.at(-1) as unknown[])[0]);
	const banner = new DOMParser().parseFromString(html, "text/html").querySelector(".run-banner");
	(plugin as { onunload(): void }).onunload();
	return { host, banner };
}

describe("mdkb dashboard re-embed action (plugin activation, mock host)", () => {
	// Catches: the action drops --reembed, so a model change silently regenerates nothing.
	it("invokes the explicit regeneration command", async () => {
		const { host } = await runEmbed(JSON.stringify({ generated: 3, skipped: 1, errors: [] }));
		expect(host.execCli).toHaveBeenCalledWith("mdkb", ["--format", "json", "embed", "--reembed"], "/repo");
	});

	// Catches: generated/skipped counts are dropped from the result banner.
	it("reports generated and skipped counts on success", async () => {
		const { banner } = await runEmbed(JSON.stringify({ generated: 3, skipped: 1, errors: [] }));
		expect(banner?.className).toContain("ok");
		expect(banner?.textContent).toContain("3 generated, 1 skipped");
	});

	// Catches: producer errors[] are ignored and a partial regeneration shows a green check.
	it("a non-empty errors list is shown as incomplete, not success", async () => {
		const { banner } = await runEmbed(
			JSON.stringify({ generated: 2, skipped: 0, errors: ["chunk 7: encoder failed"] }),
		);
		expect(banner?.className).toContain("err");
		expect(banner?.textContent).toContain("1 errors");
		expect(banner?.textContent).toContain("chunk 7: encoder failed");
	});
});

// Producer captures: real `mdkb --format json stats` / `embed --reembed` output; provenance in
// mdkbDashboard.fixtures/PROVENANCE.md. Never hand-write these files.
const fixtureDir = join(__dirname, "mdkbDashboard.fixtures");
const fixture = (name: string) => readFileSync(join(fixtureDir, name), "utf8");

describe("mdkb dashboard against recorded mdkb output", () => {
	// Catches: a producer rename of embedding keys leaves the renderer empty or showing "undefined".
	it("renders a current store from recorded stats", () => {
		const stats = JSON.parse(fixture("stats-current.json"));
		const rows = embeddingRows(stats);
		expect(rows).toHaveLength(2);
		expect(rows.join("")).not.toContain("undefined");
		expect(rows[0]).toContain(stats.embedding.selected_model);
		expect(rows.every((r) => r.includes("Current"))).toBe(true);
	});

	// Catches: a stale code archive and stale documents are not both flagged after a model switch.
	it("flags both stores after the selected model changes", () => {
		const rows = embeddingRows(JSON.parse(fixture("stats-stale.json")));
		expect(rows.every((r) => r.includes("Re-embedding required"))).toBe(true);
		expect(rows[0]).toContain("AllMiniLML6V2");
	});

	// Catches: the stale hint stays after regeneration, or Current is not restored.
	it("shows Current after the recorded re-embed", () => {
		const rows = embeddingRows(JSON.parse(fixture("stats-after-reembed.json")));
		expect(rows.every((r) => r.includes("Current"))).toBe(true);
	});

	// Catches: the recorded embed shape stops matching the banner summary.
	it("summarises the recorded embed result", async () => {
		const out = fixture("embed-reembed.json");
		const r = JSON.parse(out);
		const { banner } = await runEmbed(out);
		expect(banner?.className).toContain("ok");
		expect(banner?.textContent).toContain(`${r.generated} generated, ${r.skipped} skipped`);
	});
});
