import ts from "@typescript/typescript6";
import { describe, expect, it } from "vitest";
import { sourceFormats } from "../../utils/sourceFormats";

/**
 * Three CanvasTerminal hot paths (story #4fcc), all closure-internal state
 * (`rowMap`, `hoveredLink`, `wrapCandidateRows`, etc.) that only exists inside
 * the component's `onMount` closure — reaching them needs the full
 * canvas/transport mock stack, same as `canvasTerminalMountGuards.test.ts`.
 * A source scan states the control-flow assertion directly instead.
 */
describe.each(sourceFormats("src/components/Terminal/CanvasTerminal.tsx"))(
	"CanvasTerminal hot paths: %s",
	(_format, source) => {
		describe("resize repaint", () => {
			/**
			 * `remeasure()` always resets the canvas bitmap (`canvasRef.width = ...`)
			 * before this block runs, and `resize_pty` is async — so if this block
			 * clears `rowMap` before the immediate `paintFrame(currentFrame, m)`
			 * below, that repaint has nothing to draw from and flashes a blank grid
			 * for one frame while the new geometry's frame is in flight. The old
			 * frame must stay on screen until the real one arrives.
			 */
			it("does not clear the row cache before repainting on a PTY resize", () => {
				const start = source.indexOf("(cols !== lastResizeCols || rows !== lastResizeRows)");
				expect(start).toBeGreaterThan(-1);
				const end = source.indexOf('.catch(ipcErr("resize_pty"));', start);
				expect(end).toBeGreaterThan(start);
				const resizeBlock = source.slice(start, end);
				expect(resizeBlock).not.toMatch(/rowMap\.clear\(\)/);
			});

			it("still forces an immediate repaint of the (stale) current frame after resizing", () => {
				const start = source.indexOf("(cols !== lastResizeCols || rows !== lastResizeRows)");
				expect(start).toBeGreaterThan(-1);
				const repaint = source.indexOf("paintFrame(currentFrame, m);", start);
				expect(repaint).toBeGreaterThan(start);
				const afterBlock = source.slice(start, repaint + "paintFrame(currentFrame, m);".length);
				expect(afterBlock).toMatch(
					/if \(currentFrame\) \{\s*fullRepaintNeeded = true;\s*paintFrame\(currentFrame, m\);/,
				);
			});
		});

		describe("link probe", () => {
			/**
			 * The mousemove listener is on `document` (see the comment directly above
			 * it), so every mounted pane sees every move. Without a rect test, moving
			 * the pointer over ONE pane ran `checkLinksAtRow` — up to 3 IPC round
			 * trips (`terminal_hyperlink_span`, `terminal_get_row_text`,
			 * `resolve_terminal_path`/`terminal_get_logical_line`) — in every OTHER
			 * visible pane too.
			 */
			it("only schedules link detection for the pane under the pointer", () => {
				const start = source.indexOf("// Link detection (throttled)");
				expect(start).toBeGreaterThan(-1);
				const end = source.indexOf("scheduleLinkProbe(e)", start);
				expect(end).toBeGreaterThan(start);
				const block = source.slice(start, end);
				expect(block).toMatch(/isPointerInsideRect\(e, canvasRef\.getBoundingClientRect\(\)\)/);
			});
		});

		describe("wrapped-link verification batching", () => {
			/**
			 * The multi-row pass used to await `terminal_get_logical_line` for one
			 * wrap-candidate row at a time inside a `for` loop, so a screen with N
			 * candidates cost N serial round trips before any of it resolved. The
			 * single-row pass right above it already fixed the equivalent problem for
			 * `resolve_terminal_path` by batching into one `resolve_terminal_paths`
			 * call — this applies the same fix here.
			 */
			it("fetches every wrap-candidate row's logical line concurrently", () => {
				const start = source.indexOf("const wrapCandidateRows: number[] = [];");
				expect(start).toBeGreaterThan(-1);
				const end = source.indexOf("if (fileCandidates.length > 0)", start);
				expect(end).toBeGreaterThan(start);
				const block = source.slice(start, end);
				expect(block).toMatch(/await Promise\.all\(\s*wrapCandidateRows\.map/);
			});

			it("resolves every wrapped file:// candidate in one batched call, not one per match", () => {
				// Catches per-match resolution without depending on the repaint guard's spelling.
				const file = ts.createSourceFile("CanvasTerminal.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
				const batches: ts.IfStatement[] = [];
				const findBatch = (node: ts.Node) => {
					if (ts.isIfStatement(node) && /^wrapCandidateRows\.length\s*>\s*0$/.test(node.expression.getText(file))) {
						batches.push(node);
					}
					ts.forEachChild(node, findBatch);
				};
				findBatch(file);
				expect(batches).toHaveLength(1);
				const resolvers: string[] = [];
				const findResolvers = (node: ts.Node) => {
					if (ts.isCallExpression(node)) {
						const command = node.arguments[0];
						if (command && ts.isStringLiteral(command) && /^resolve_terminal_paths?$/.test(command.text)) {
							resolvers.push(command.text);
						}
					}
					ts.forEachChild(node, findResolvers);
				};
				findResolvers(batches[0].thenStatement);
				expect(resolvers).toEqual(["resolve_terminal_paths"]);
			});
		});
	},
);
