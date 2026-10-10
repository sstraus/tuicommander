import { expect, it, vi } from "vitest";
import { automationAdapter } from "../../components/AutomationsDialog/transportAdapter";
import { invoke } from "../../invoke";

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue({ ok: true }) }));
it.each([
	[false, "pause"],
	[true, "resume"],
] as const)("enabled=%s sends only id so stale fields cannot overwrite persisted edits", async (enabled, action) => {
	vi.mocked(invoke).mockClear();
	await automationAdapter.setEnabled("a1", enabled);
	expect(invoke).toHaveBeenCalledWith("automation_action", { input: { action, id: "a1" } });
});
