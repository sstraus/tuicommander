import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { rpc } from "../../transport";
import { TerminalKeybar } from "../components/TerminalKeybar";
import styles from "../components/TerminalKeybar.module.css";

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

	it("uses Codex's accepted submit byte instead of the ignored modified Enter", async () => {
		const view = render(() => <TerminalKeybar sessionId="owned" agentType="codex" />);
		fireEvent.click(view.getByRole("button", { name: "Ctrl" }));
		fireEvent.click(view.getByRole("menuitem", { name: "Ctrl+Enter" }));
		await waitFor(() => expect(rpc).toHaveBeenCalledExactlyOnceWith("write_pty", { sessionId: "owned", data: "\r" }));
	});

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

	it("marks destructive interrupt and EOF choices as danger without marking background or submit", () => {
		const view = render(() => <TerminalKeybar sessionId="owned" />);
		fireEvent.click(view.getByRole("button", { name: "Ctrl" }));
		for (const label of ["Ctrl+C", "Ctrl+D"])
			expect(view.getByRole("menuitem", { name: label })).toHaveClass(styles.danger);
		for (const label of ["Ctrl+B", "Ctrl+Enter"])
			expect(view.getByRole("menuitem", { name: label })).not.toHaveClass(styles.danger);
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
