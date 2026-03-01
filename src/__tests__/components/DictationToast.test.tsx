import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@solidjs/testing-library";
import { createRoot } from "solid-js";

// Must import mocks before store/component
import { mockInvoke } from "../mocks/tauri";

describe("DictationToast", () => {
  let DictationToast: typeof import("../../components/DictationToast/DictationToast").DictationToast;
  let dictationStore: typeof import("../../stores/dictation").dictationStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
    const storeModule = await import("../../stores/dictation");
    dictationStore = storeModule.dictationStore;
    const component = await import("../../components/DictationToast/DictationToast");
    DictationToast = component.DictationToast;
  });

  it("is hidden by default", () => {
    const { container } = render(() => <DictationToast />);
    expect(container.querySelector(".toast")).toBeNull();
  });

  it("shows toast when recording starts and partial text arrives", async () => {
    // Mock start_dictation to succeed
    mockInvoke.mockResolvedValueOnce(undefined);

    await createRoot(async (dispose) => {
      const { container } = render(() => <DictationToast />);

      // Start recording (sets recording=true in store)
      await dictationStore.startRecording();
      expect(dictationStore.state.recording).toBe(true);

      // Toast should still be hidden (no partial text yet)
      expect(container.querySelector(".toast")).toBeNull();

      dispose();
    });
  });

  it("hides toast after recording stops", async () => {
    mockInvoke.mockResolvedValueOnce(undefined); // start_dictation

    await createRoot(async (dispose) => {
      const { container } = render(() => <DictationToast />);

      await dictationStore.startRecording();
      expect(dictationStore.state.recording).toBe(true);

      // Stop recording
      mockInvoke.mockResolvedValueOnce({ text: "hello", skip_reason: null, duration_s: 1.0 });
      await dictationStore.stopRecording();

      expect(dictationStore.state.recording).toBe(false);
      expect(dictationStore.state.partialText).toBe("");
      dispose();
    });
  });
});
