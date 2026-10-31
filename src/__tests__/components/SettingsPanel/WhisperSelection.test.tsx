import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { SettingsPanel } from "../../../components/SettingsPanel/SettingsPanel";
import { appLogger } from "../../../stores/appLogger";
import { dictationStore, type ModelInfo } from "../../../stores/dictation";
import { mockInvoke } from "../../mocks/tauri";

afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

it("keeps the configured Whisper choice when the async catalogue replaces its fallback in SettingsPanel", async () => {
	// Catches: the native select switching to its first enabled option while
	// the real store still names the configured model after catalogue arrival.
	let finishCatalogue!: (models: ModelInfo[]) => void;
	const catalogue = new Promise<ModelInfo[]>((resolve) => {
		finishCatalogue = resolve;
	});
	const errors = vi.spyOn(appLogger, "error").mockImplementation(() => {});
	mockInvoke.mockImplementation(async (command: string) => {
		switch (command) {
			case "get_model_info":
				return catalogue;
			case "get_dictation_config":
				return { enabled: false, hotkey: "F5", language: "auto", model: "large-v3-turbo", speech_engine: "edge" };
			case "get_dictation_status":
				return { model_status: "not_downloaded", model_name: "large-v3-turbo", model_size_mb: 0 };
			case "get_speech_assets":
				return [];
			case "get_hands_free_status":
				return { armed: false, phase: "disarmed", sessionId: null, owner: null };
			case "get_speech_status":
				return null;
			case "check_microphone_permission":
				return "not_determined";
			default:
				return {};
		}
	});

	const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="dictation" />);
	const select = view.getByRole("combobox", { name: "Whisper model" }) as HTMLSelectElement;
	expect(select.value).toBe("large-v3-turbo");
	finishCatalogue([
		{ name: "small", display_name: "Small", size_hint_mb: 488, downloaded: true, actual_size_mb: 488 },
		{
			name: "large-v3-turbo",
			display_name: "Large V3 Turbo",
			size_hint_mb: 1620,
			downloaded: false,
			actual_size_mb: 0,
		},
	]);
	await waitFor(() => expect(view.getByRole("option", { name: "Large V3 Turbo" })).toBeDefined());
	expect(dictationStore.state.selectedModel).toBe("large-v3-turbo");
	expect(select.value).toBe("large-v3-turbo");
	expect((view.getByRole("option", { name: "Large V3 Turbo" }) as HTMLOptionElement).disabled).toBe(true);
	expect(mockInvoke.mock.calls.some(([command]) => command === "save_dictation_config")).toBe(false);
	expect(errors).not.toHaveBeenCalled();
});
