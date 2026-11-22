import { describe, expect, it } from "vitest";
import { buildAnswersTurn } from "../answersTurn";
import {
	cachedMarkerSnapshot,
	createCanvasMarkerPlanner,
	planCanvasMarkers,
	readMarkerPromptBoundary,
} from "../canvasTerminalMarkers";
import type { RowSnapshot } from "../suggestOverlay";
import claude from "./fixtures/claude-prompt-answer-1389.json";
import codex from "./fixtures/codex-answer-paragraphs-1872.json";

const snapshots = (texts: string[]): RowSnapshot[] => texts.map((text) => ({ text, isWrapped: false }));

describe("CanvasTerminal bounded viewport marker planner", () => {
	// Catches: a retained prompt prefix makes one paint walk the entire scrollback.
	it("bounds a 50000-row prompt ownership plan to the viewport and lookback", () => {
		let reads = 0;
		const viewport = 49960;
		const started = performance.now();
		const plan = planCanvasMarkers(
			40,
			(i) => {
				reads++;
				const absolute = viewport + i;
				return absolute < 0 || absolute >= 50000
					? null
					: {
							text: absolute === 0 ? "❯ pasted question" : "  pasted context",
							isWrapped: false,
						};
			},
			[-viewport],
		);
		const elapsed = performance.now() - started;
		console.info(`marker-plan fixture: reads=${reads} duration_ms=${elapsed.toFixed(3)}`);
		expect(plan.answerMarkers.size).toBe(0);
		expect(reads).toBeLessThanOrEqual(200);
	});

	// Catches: unchanged paints decode the same retained rows, and stale cached text survives a row replacement.
	it("classifies many-answer history lazily and reuses unchanged row generations", () => {
		const history = snapshots(
			Array.from({ length: 50000 }, (_, i) => (i % 4 === 0 ? "⏺ 💬 Answer" : "  continuation")),
		);
		let accesses = 0;
		const snapshot = cachedMarkerSnapshot((row: RowSnapshot) => {
			accesses++;
			return row.text;
		});
		const planner = createCanvasMarkerPlanner();
		let base = 49959;
		const getRow = (i: number) => (history[base + i] ? snapshot(history[base + i], false) : null);
		const started = performance.now();
		const first = planner(40, getRow);
		const coldReads = accesses;
		const coldMs = performance.now() - started;
		expect(first.answerMarkers.size).toBeGreaterThan(0);
		expect(coldReads).toBeLessThanOrEqual(170);
		const warmStart = performance.now();
		expect(planner(40, getRow)).toBe(first);
		console.info(
			`many-answer fixture: cold_reads=${coldReads} cold_ms=${coldMs.toFixed(3)} warm_ms=${(performance.now() - warmStart).toFixed(3)} warm_reads=${accesses - coldReads}`,
		);
		expect(accesses).toBe(coldReads);
		history[base] = { text: "intent: Replaced row", isWrapped: false };
		expect(planner(40, getRow).blocks).toContainEqual({ row: 0, kind: "intent" });
		expect(accesses).toBe(coldReads + 1);
		base -= 40000;
		expect(planner(40, getRow).answerMarkers.size).toBeGreaterThan(0);
		expect(accesses - coldReads - 1).toBeLessThanOrEqual(170);
	});

	// Catches: a predecessor wrap change leaves a cached answer marker on a soft-wrapped row.
	it("invalidates the plan when only the predecessor wrap interpretation changes", () => {
		let reads = 0;
		const row = { text: "  💬 Answer" };
		const snapshot = cachedMarkerSnapshot((value: typeof row) => {
			reads++;
			return value.text;
		});
		const planner = createCanvasMarkerPlanner();
		let wrapped = false;
		const getRow = (i: number) => (i === 0 ? snapshot(row, wrapped) : null);
		expect([...planner(1, getRow).answerMarkers]).toEqual([0]);
		wrapped = true;
		expect([...planner(1, getRow).answerMarkers]).toEqual([]);
		expect(reads).toBe(1);
	});

	// Catches: a background ownership read sweeps the entire retained prompt in a single repaint task.
	it("resumes ownership reads with bounded work and preserves a boundary blank", async () => {
		const rows = snapshots(["❯ Question", ...Array.from({ length: 126 }, () => "  context"), "", "  💬 Answer"]);
		const reads: [number, number][] = [];
		const read = async (from: number, to: number) => {
			reads.push([from, to]);
			return rows.slice(from, to);
		};
		const first = await readMarkerPromptBoundary(0, rows.length, read, 128);
		expect(first).toEqual({ end: 128, resolved: false });
		expect(await readMarkerPromptBoundary(first!.end - 1, rows.length, read, 128)).toEqual({
			end: 128,
			resolved: true,
		});
		expect(reads).toEqual([
			[0, 128],
			[127, 129],
		]);
	});

	// Catches: a quoted marker becomes an answer when the submitted prompt is outside the 32-row lookback.
	it("retains recorded Claude prompt ownership above the viewport lookback", () => {
		const prompt = claude.rows.findIndex((text) => text.startsWith("❯"));
		const rows = snapshots(claude.rows.slice(prompt));
		rows.splice(1, 0, ...snapshots(Array.from({ length: 60 }, () => "  pasted context")));
		const viewport = rows.findIndex((row) => row.text.startsWith("  💬"));
		const answer = rows.findIndex((row) => row.text.startsWith("⏺ 💬"));
		expect(viewport).toBeGreaterThan(32);
		const plan = planCanvasMarkers(rows.length - viewport, (i) => rows[viewport + i] ?? null, [-viewport]);
		expect([...plan.answerMarkers]).toEqual([answer - viewport]);
		expect(plan.blocks.filter((block) => block.kind === "answer").map((block) => block.row)).toEqual(
			Array.from({ length: 7 }, (_, i) => answer - viewport + i),
		);
	});

	// Catches: prompt ownership swallows a genuine indented answer separated from the prompt by a blank row.
	it("keeps a blank-separated indented answer in the gutter and extracted turn", () => {
		const rows = snapshots(["⏺ 💬 First.", "", "  ❯ Next prompt", "", "  💬 Second."]);
		expect([...planCanvasMarkers(rows.length, (i) => rows[i] ?? null).answerMarkers]).toEqual([0, 4]);
		expect(buildAnswersTurn(rows.slice(2), true)).toEqual({ prompt: "  ❯ Next prompt", answers: ["💬 Second."] });
	});

	// Catches: recognizing only Claude's chevron lets Codex prompt text participate in answer classification.
	it("keeps the recorded Codex prompt separate from its marked answer paragraphs", () => {
		const rows = snapshots(codex.rows);
		const prompt = codex.rows.findIndex((text) => text.startsWith("›"));
		const plan = planCanvasMarkers(rows.length, (i) => rows[i] ?? null, [prompt]);
		expect([...plan.answerMarkers]).toEqual([18, 20, 22]);
		expect(buildAnswersTurn(rows.slice(prompt, 25), false).answers).toEqual([
			"💬 The answer highlight runs continuously across the full response.\n\n💬 Blank paragraph spacing stays preserved.\n\n💬 Raw copy text remains untouched.",
		]);
	});
	// Catches: a cold paint cache fails open on a quoted marker instead of reading the known prompt prefix.
	it("reads missing prompt ownership before classifying a cold viewport", async () => {
		const prompt = claude.rows.findIndex((text) => text.startsWith("❯"));
		const rows = snapshots(claude.rows.slice(prompt));
		rows.splice(1, 0, ...snapshots(Array.from({ length: 60 }, () => "  pasted context")));
		const viewport = rows.findIndex((row) => row.text.startsWith("  💬"));
		const getRow = (i: number) => (i < -32 ? null : (rows[viewport + i] ?? null));
		const pending = planCanvasMarkers(rows.length - viewport, getRow, [-viewport]);
		expect(pending.pendingPrompt).toBe(-viewport);
		expect([...pending.answerMarkers]).toEqual([]);
		const boundary = await readMarkerPromptBoundary(0, rows.length, async (from, to) => rows.slice(from, to));
		expect(boundary).toEqual({ end: viewport + 8, resolved: true });
		const ready = planCanvasMarkers(rows.length - viewport, getRow, [-viewport], {
			end: boundary!.end - viewport,
			resolved: boundary!.resolved,
		});
		expect([...ready.answerMarkers]).toEqual([8]);
	});

	// Catches: missing the old output bullet suppresses genuine marked continuation paragraphs in a later viewport.
	it("releases an offscreen prompt when output started before the lookback", async () => {
		const rows = snapshots([
			"❯ Question",
			"⏺ First output",
			...Array.from({ length: 80 }, () => "  continuation"),
			"  💬 Answer",
		]);
		const viewport = rows.length - 1;
		const boundary = await readMarkerPromptBoundary(0, rows.length, async (from, to) => rows.slice(from, to));
		const plan = planCanvasMarkers(1, (i) => (i < -32 ? null : (rows[viewport + i] ?? null)), [-viewport], {
			end: boundary!.end - viewport,
			resolved: boundary!.resolved,
		});
		expect([...plan.answerMarkers]).toEqual([0]);
	});

	// Catches: a chunk edge turns a soft-wrapped quoted marker into a new output marker.
	it("preserves wrapped prompt markers across ownership-read chunks", async () => {
		const rows = snapshots([
			"› Question",
			...Array.from({ length: 254 }, () => "  context"),
			"",
			"  💬 quoted",
			"• 💬 Answer",
		]);
		rows[256].isWrapped = true;
		const reads: [number, number][] = [];
		expect(
			await readMarkerPromptBoundary(0, rows.length, async (from, to) => {
				reads.push([from, to]);
				return rows.slice(from, to);
			}),
		).toEqual({ end: 257, resolved: true });
		expect(reads).toEqual([
			[0, 256],
			[255, 258],
		]);
	});

	// Catches: rescanning the accumulated prefix makes long-prompt classification quadratic.
	it("classifies each ownership-read chunk once instead of rescanning the prefix", async () => {
		let accesses = 0;
		const count = 8192;
		const rows = Array.from({ length: count }, () => ({
			get text() {
				accesses++;
				return "  pasted context";
			},
			isWrapped: false,
		}));
		expect(await readMarkerPromptBoundary(0, count, async (from, to) => rows.slice(from, to))).toEqual({
			end: count,
			resolved: false,
		});
		// Each nonwrapped row needs at most the output and answer-marker checks.
		expect(accesses).toBeLessThanOrEqual(count * 2);
	});

	// Catches: incremental chunk scans forget the preceding blank row at the boundary.
	it("recognizes blank-separated output at the first row of a later chunk", async () => {
		const rows = snapshots(["❯ Question", ...Array.from({ length: 254 }, () => "  context"), "", "  💬 Answer"]);
		expect(await readMarkerPromptBoundary(0, rows.length, async (from, to) => rows.slice(from, to))).toEqual({
			end: 256,
			resolved: true,
		});
	});

	// Catches: a failed ownership read is promoted to an empty prompt and exposes quoted markers.
	it("does not classify a failed prompt read as an output boundary", async () => {
		expect(await readMarkerPromptBoundary(0, 100, async () => null)).toBeNull();
	});

	// Catches: Codex's different prompt glyph exposes a quoted hard-line marker as an answer.
	it.each(["❯", "›"])("keeps an uninterrupted quoted marker inside a %s prompt", (glyph) => {
		const rows = snapshots([`${glyph} Question`, "  quoted preface", "  💬 Quoted example", "• 💬 Answer"]);
		expect([...planCanvasMarkers(rows.length, (i) => rows[i] ?? null).answerMarkers]).toEqual([3]);
		expect(buildAnswersTurn(rows, false).answers).toEqual(["💬 Answer"]);
	});
});

describe("CanvasTerminal marker plan while a prompt-boundary read is pending", () => {
	// One turn: a two-row prompt, then output with two marked answers.
	const turn = snapshots([
		"❯ Question",
		"  more question text",
		"",
		"⏺ 💬 First answer",
		...Array.from({ length: 40 }, () => "  continuation"),
		"⏺ 💬 Second answer",
		...Array.from({ length: 10 }, () => "  tail"),
	]);
	const ROWS = 15;
	// The paint cache keeps 32 rows above the viewport, as in CanvasTerminal.
	const cachedFrom = (base: number) => (i: number) => (i < -32 ? null : (turn[base + i] ?? null));

	async function resolvedPlan(base: number) {
		const boundary = await readMarkerPromptBoundary(0, turn.length, async (from, to) => turn.slice(from, to));
		return planCanvasMarkers(ROWS, (i) => turn[base + i] ?? null, [-base], {
			end: boundary!.end - base,
			resolved: boundary!.resolved,
		});
	}

	// Catches: output scrolls the prompt out of the cached window, the pending read paints a plan with no
	// marked rows, and every answer gutter and hidden marker row flips off and back on (one jump per line).
	it("keeps the last resolved plan, moved by the scroll, until the read resolves", async () => {
		const planner = createCanvasMarkerPlanner();
		const before = planner(ROWS, cachedFrom(30), [-30], undefined, { base: 30, generation: 1 });
		expect(before.pendingPrompt).toBeUndefined();
		expect(before.answerMarkers.has(14)).toBe(true);

		const during = planner(ROWS, cachedFrom(34), [-34], undefined, { base: 34, generation: 1 });
		const expected = await resolvedPlan(34);
		expect(during.pendingPrompt).toBe(-34);
		expect([...during.answerMarkers].sort((a, b) => a - b)).toEqual([...expected.answerMarkers].sort((a, b) => a - b));
		expect([...during.hidden]).toEqual([...expected.hidden]);
	});

	// Catches: a plan held across a cache reset (clear or reflow) hides rows that no longer hold markers.
	it("does not hold a plan across a cache generation change", () => {
		const planner = createCanvasMarkerPlanner();
		planner(ROWS, cachedFrom(30), [-30], undefined, { base: 30, generation: 1 });
		const during = planner(ROWS, cachedFrom(34), [-34], undefined, { base: 34, generation: 2 });
		expect(during.pendingPrompt).toBe(-34);
		expect([...during.answerMarkers]).toEqual([]);
	});
});
