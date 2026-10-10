import { readFileSync } from "node:fs";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import agentKeyFixture from "../../../docs/evidence/ctrl-menu-1660/agent-key-fixture.json";
import { AGENT_TYPES } from "../../agents";
import { rpc } from "../../transport";
import { TerminalKeybar } from "../components/TerminalKeybar";

const keybarCss = readFileSync(`${process.cwd()}/src/mobile/components/TerminalKeybar.module.css`, "utf8");

vi.mock("../../transport", () => ({ rpc: vi.fn(() => Promise.resolve()), HttpRpcError: class extends Error {} }));
afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

describe("mobile control menu", () => {
	it.each([
		["Ctrl+C", "\x03"],
		["Ctrl+B", "\x02"],
		["Ctrl+D", "\x04"],
		["Ctrl+Enter", "\x1b[13;5u"],
	])("sends %s exactly once and closes, preventing wrong bytes or duplicate input", async (label, data) => {
		const view = render(() => <TerminalKeybar sessionId="owned" agentType="claude" />);
		expect(view.queryByRole("menu")).toBeNull();
		fireEvent.click(view.getByRole("button", { name: "Ctrl" }));
		fireEvent.click(view.getByRole("menuitem", { name: label }));
		await waitFor(() => expect(rpc).toHaveBeenCalledExactlyOnceWith("write_pty", { sessionId: "owned", data }));
		expect(view.queryByRole("menu")).toBeNull();
	});

	it("covers every registered agent so additions cannot silently inherit an unsafe fallback", () => {
		expect(agentKeyFixture.map((row) => row.agent).sort()).toEqual([...AGENT_TYPES].sort());
	});

	it.each(agentKeyFixture)(
		"uses recorded mapping for $agent, preventing ignored or corrupted Ctrl+Enter",
		async ({ agent, selectedSequence }) => {
			const view = render(() => <TerminalKeybar sessionId="owned" agentType={agent} />);
			fireEvent.click(view.getByRole("button", { name: "Ctrl" }));
			fireEvent.click(view.getByRole("menuitem", { name: "Ctrl+Enter" }));
			await waitFor(() =>
				expect(rpc).toHaveBeenCalledExactlyOnceWith("write_pty", { sessionId: "owned", data: selectedSequence }),
			);
		},
	);

	it.each([null, undefined, "unknown", "__proto__"])(
		"preserves modified Enter for unknown %s instead of executing an accidental shell command",
		async (agentType) => {
			const view = render(() => <TerminalKeybar sessionId="owned" agentType={agentType} />);
			fireEvent.click(view.getByRole("button", { name: "Ctrl" }));
			fireEvent.click(view.getByRole("menuitem", { name: "Ctrl+Enter" }));
			await waitFor(() =>
				expect(rpc).toHaveBeenCalledExactlyOnceWith("write_pty", { sessionId: "owned", data: "\x1b[13;5u" }),
			);
		},
	);

	it("dismisses with outside tap, toggle or Escape without accidentally sending", () => {
		const view = render(() => <TerminalKeybar sessionId="owned" />);
		const ctrl = view.getByRole("button", { name: "Ctrl" });
		for (const dismiss of [
			() => fireEvent.click(document.body),
			() => fireEvent.click(ctrl),
			() => fireEvent.keyDown(document, { key: "Escape" }),
		]) {
			fireEvent.click(ctrl);
			expect(view.getAllByRole("menuitem")).toHaveLength(4);
			dismiss();
			expect(view.queryByRole("menu")).toBeNull();
		}
		expect(rpc).not.toHaveBeenCalled();
	});

	it("renders interrupt and EOF in the error colour, catching a missing or broken danger rule", () => {
		const stylesheet = document.createElement("style");
		stylesheet.textContent = ":root { --error: rgb(255, 0, 0); --fg-secondary: rgb(128, 128, 128); }" + keybarCss;
		document.head.append(stylesheet);
		try {
			const view = render(() => <TerminalKeybar sessionId="owned" />);
			fireEvent.click(view.getByRole("button", { name: "Ctrl" }));
			for (const label of ["Ctrl+C", "Ctrl+D"])
				expect(getComputedStyle(view.getByRole("menuitem", { name: label })).color).toBe("rgb(255, 0, 0)");
			for (const label of ["Ctrl+B", "Ctrl+Enter"])
				expect(getComputedStyle(view.getByRole("menuitem", { name: label })).color).toBe("rgb(128, 128, 128)");
		} finally {
			stylesheet.remove();
		}
	});

	it("closes on another keybar key and sends Tab once instead of leaving the menu open", async () => {
		const view = render(() => <TerminalKeybar sessionId="owned" />);
		fireEvent.click(view.getByRole("button", { name: "Ctrl" }));
		fireEvent.click(view.getByRole("button", { name: "Tab" }));
		expect(view.queryByRole("menu")).toBeNull();
		await waitFor(() => expect(rpc).toHaveBeenCalledExactlyOnceWith("write_pty", { sessionId: "owned", data: "\t" }));
	});

	it("prevents control writes when the session is unavailable", () => {
		const view = render(() => <TerminalKeybar sessionId="owned" sessionExists={false} />);
		const ctrl = view.getByRole("button", { name: "Ctrl" });
		expect(ctrl).toBeDisabled();
		fireEvent.click(ctrl);
		expect(view.queryByRole("menu")).toBeNull();
		expect(rpc).not.toHaveBeenCalled();
	});
});
