// @vitest-environment node
import { execFile } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { promisify } from "node:util";
import { expect, it } from "vitest";

const exec = promisify(execFile);
const wrapper = process.env.TUIC_SIDEBAR_BROWSER;
const url = process.env.TUIC_SIDEBAR_URL;

// Opt-in real-browser component check. DOM emulators cannot reproduce native
// focus scrolling or CSS overflow. Start worktree Vite, then set
// TUIC_SIDEBAR_URL=http://127.0.0.1:<port>/src/__tests__/fixtures/sidebarResize.html
// and TUIC_SIDEBAR_BROWSER=<stealth wrapper>. The suite owns only its named tab.
it.skipIf(!wrapper || !url)(
	"keeps row starts visible when overflowing actions are focused before and after widening",
	async () => {
		const evidence = resolve(process.env.TUIC_SIDEBAR_EVIDENCE ?? ".tmp/sidebar-resize-browser");
		await mkdir(evidence, { recursive: true });
		const run = async (...args: string[]) => {
			const { stdout } = await exec(wrapper!, args, {
				env: { ...process.env, AB_SESSION: `tuic-sidebar-resize-${process.pid}` },
				timeout: 30000,
				maxBuffer: 2e6,
			});
			return stdout;
		};
		const evaluate = async (code: string) => JSON.parse(await run("--", "eval", code));
		interface Measurement {
			width: number;
			scrollLeft: number;
			nameX: number;
			actionRight: number;
			contentRight: number;
			vertical: boolean;
			rowHeight: number;
		}
		const proof: Array<{ density: string; badges: number; before: Measurement; after: Measurement; calls: unknown }> =
			[];
		try {
			// Catches crowded compact facts, rich text scaling, and an ordinary row
			// losing its native action targets or vertical scrolling after containment.
			for (const [density, badges] of [
				["compact", 1],
				["rich", 1],
				["compact", 0],
			] as const) {
				await run(`${url}?density=${density}&badges=${badges}`);
				await run("--", "wait", '[aria-label="Remove worktree"]');
				const row = await evaluate(
					`(() => {const b=document.querySelector('[aria-label="Remove worktree"]');if(!b)throw Error('fixture not mounted');const r=b.closest('[class*="branchItem"]').getBoundingClientRect();return {x:r.x+100,y:r.y+r.height/2};})()`,
				);
				await run("--", "mouse", "move", String(Math.round(row.x)), String(Math.round(row.y)));
				const sample = `(() => {document.body.offsetWidth;document.getAnimations().forEach(a=>a.finish());document.body.offsetWidth;const sidebar=document.querySelector('#sidebar');const c=sidebar.firstElementChild;const b=document.querySelector('[aria-label="Remove worktree"]');const row=b.closest('[class*="branchItem"]');return {width:sidebar.getBoundingClientRect().width,scrollLeft:c.scrollLeft,scrollWidth:c.scrollWidth,clientWidth:c.clientWidth,nameX:document.querySelector('[class*="repoName"]').getBoundingClientRect().x,actionRight:b.getBoundingClientRect().right,contentRight:c.getBoundingClientRect().right,rowHeight:row.getBoundingClientRect().height,vertical:c.scrollHeight>c.clientHeight};})()`;
				await evaluate(
					`(() => {document.body.offsetWidth;document.getAnimations().forEach(a=>a.finish());document.body.offsetWidth;document.activeElement.blur();document.querySelector('[aria-label="Remove worktree"]').focus();return true;})()`,
				);
				const before = await evaluate(sample);
				// Drive the real resize entry point; do not assign width or scrollLeft.
				await evaluate(
					`(() => {const h=document.querySelector('[class*="resizeHandle"]');h.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,clientX:297}));document.dispatchEvent(new MouseEvent('mousemove',{clientX:317}));document.dispatchEvent(new MouseEvent('mouseup'));return true;})()`,
				);
				await run("--", "mouse", "move", String(Math.round(row.x)), String(Math.round(row.y)));
				await evaluate(
					`(() => {document.body.offsetWidth;document.getAnimations().forEach(a=>a.finish());document.body.offsetWidth;document.activeElement.blur();document.querySelector('[aria-label="Remove worktree"]').focus();return true;})()`,
				);
				const after = await evaluate(sample);
				await evaluate(
					`(() => {const b=document.querySelector('[aria-label="Remove worktree"]');const row=b.closest('[class*="branchItem"]');b.click();row.querySelector('[aria-label="Add terminal"]').click();return true;})()`,
				);
				const calls = await evaluate("window.sidebarResizeCalls");
				// A final wide view records the user-visible result, not just labels in a DOM emulator.
				await evaluate(
					`(() => {const h=document.querySelector('[class*="resizeHandle"]');h.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,clientX:317}));document.dispatchEvent(new MouseEvent('mousemove',{clientX:497}));document.dispatchEvent(new MouseEvent('mouseup'));document.body.offsetWidth;document.getAnimations().forEach(a=>a.finish());document.body.offsetWidth;return true;})()`,
				);
				await run("--", "screenshot", resolve(evidence, `${density}-${badges}-500px.png`));
				proof.push({ density, badges, before, after, calls });
				await writeFile(resolve(evidence, "proof.json"), JSON.stringify(proof, null, 2));
			}
			await writeFile(resolve(evidence, "proof.json"), JSON.stringify(proof, null, 2));
			for (const entry of proof) {
				for (const [phase, state] of [
					["before", entry.before],
					["after", entry.after],
				] as const) {
					expect
						.soft(state.scrollLeft, `${entry.density}/${entry.badges} ${phase}: focus must not scroll rows left`)
						.toBe(0);
					expect
						.soft(state.nameX, `${entry.density}/${entry.badges} ${phase}: repository start must remain visible`)
						.toBeGreaterThanOrEqual(0);
					expect
						.soft(state.actionRight, `${entry.density}/${entry.badges} ${phase}: action must fit the row`)
						.toBeLessThanOrEqual(state.contentRight);
					expect.soft(state.vertical).toBe(true);
					expect.soft(state.rowHeight).toBe(28);
				}
				expect.soft(entry.after.width).toBe(320);
				expect.soft(entry.calls).toEqual({ add: 1, remove: 1, select: 0 });
			}
		} catch (error) {
			await writeFile(
				resolve(evidence, "failure-page.json"),
				await run("--", "eval", "({url:location.href,html:document.body.innerHTML})"),
			);
			throw error;
		} finally {
			await run("--", "tab", "close");
		}
	},
	180000,
);
