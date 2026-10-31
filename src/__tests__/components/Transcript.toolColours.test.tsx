// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

afterEach(cleanup);

function toolRows(titles: string[]) {
	const entries: AcpTranscriptEntry[] = titles.map((title, index) => ({
		id: String(index),
		kind: "tool",
		call: { toolCallId: String(index), title, status: "completed" },
	}));
	const view = render(() => (
		<Transcript
			terminalAgent="claude"
			entries={() => entries}
			busy={() => false}
			emptyMessage=""
			observeToolDuration={false}
		/>
	));
	const group = view.container.querySelector("details")!;
	fireEvent.click(group.querySelector("summary")!);
	group.open = true;
	return { ...view, group, rows: Array.from(group.querySelectorAll(":scope > div > details > summary")) };
}

describe("expanded tool row colours", () => {
	// Catches: shell titles rendered as one grey node, including execute aliases and non-builtin commands.
	it.each(["Bash", "execute"])("colours %s command, flag, string and operator without changing text", async (tool) => {
		const title = `${tool}: pnpm --filter 'chat view' && git status`;
		const { rows } = toolRows([title]);
		await waitFor(() => expect(rows[0].querySelector('[data-tool-role="command"]')?.textContent).toBe("pnpm"));
		for (const [role, text] of [
			["flag", "--filter"],
			["string", "'chat view'"],
			["operator", "&&"],
		]) {
			const span = rows[0].querySelector(`[data-tool-role="${role}"]`)!;
			expect(span.textContent).toBe(text);
			expect(span.className).not.toBe("");
		}
		expect(rows[0].textContent).toContain(title);
		expect(
			new Set(Array.from(rows[0].querySelectorAll("[data-tool-role]")).map((span) => span.className)).size,
		).toBeGreaterThanOrEqual(4);
	});

	// Catches: file tool labels disappearing or paths being coloured as shell commands; fallback losing text.
	it("separates Read Edit Write names and paths and preserves unknown or bare tools", () => {
		const titles = [
			"Read: /repo/chat view.tsx",
			"Edit: C:\\repo\\view.tsx",
			"Write: ./view.tsx",
			"search_tools: shell <command>",
			"Read",
		];
		const { rows } = toolRows(titles);
		for (const [index, tool] of ["Read", "Edit", "Write"].entries()) {
			expect(rows[index].querySelector('[data-tool-role="tool"]')?.textContent).toBe(tool);
			expect(rows[index].querySelector('[data-tool-role="path"]')?.textContent).toBe(
				titles[index].slice(tool.length + 2),
			);
			expect(rows[index].textContent).toContain(titles[index]);
		}
		expect(rows[3].textContent).toContain(titles[3]);
		expect(rows[4].textContent).toContain("Read");
		expect(rows[3].querySelector('[data-tool-role="command"]')).toBeNull();
	});

	// Catches: highlighting the collapsed summary or interpreting quoted markup as DOM.
	it("leaves the collapsed summary plain and keeps shell markup literal", async () => {
		const title = 'Bash: echo "<img src=x>" | cat';
		const { group, rows } = toolRows([title]);
		expect(group.querySelector(":scope > summary")?.textContent).toBe(`1 tool call ›${title}Completed`);
		expect(group.querySelector(":scope > summary [data-tool-role]")).toBeNull();
		await waitFor(() => expect(rows[0].querySelector('[data-tool-role="string"]')?.textContent).toBe('"<img src=x>"'));
		expect(rows[0].querySelector("img")).toBeNull();
		expect(rows[0].textContent).toContain(title);
	});
});
