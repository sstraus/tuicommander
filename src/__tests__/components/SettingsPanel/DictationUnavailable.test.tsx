import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, expect, it } from "vitest";
import { DictationSettings } from "../../../components/SettingsPanel/DictationSettings";
import { dictationStore } from "../../../stores/dictation";
import { mockInvoke } from "../../mocks/tauri";

afterEach(cleanup);

it("shows unavailable without invoking voice commands after the missing config probe", async () => {
	mockInvoke.mockRejectedValue("Command get_dictation_config not found");
	await dictationStore.refreshConfig();
	mockInvoke.mockClear();
	const view = render(() => <DictationSettings />);
	await Promise.resolve();
	expect(view.getByText("Dictation is not available in this build")).toBeDefined();
	expect(mockInvoke).not.toHaveBeenCalled();
});
