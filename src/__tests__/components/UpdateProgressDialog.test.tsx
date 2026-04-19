import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@solidjs/testing-library";
import { createStore } from "solid-js/store";

interface MockUpdaterState {
  downloading: boolean;
  progress: number;
  version: string | null;
  error: string | null;
}

const [mockState, setMockState] = createStore<MockUpdaterState>({
  downloading: false,
  progress: 0,
  version: null,
  error: null,
});

vi.mock("../../stores/updater", () => ({
  updaterStore: {
    get state() {
      return mockState;
    },
    dismiss: vi.fn(),
  },
}));

import { UpdateProgressDialog } from "../../components/UpdateProgressDialog/UpdateProgressDialog";

describe("UpdateProgressDialog title", () => {
  beforeEach(() => {
    setMockState({ downloading: false, progress: 0, version: null, error: null });
  });

  it("shows 'Update failed' when error is set (no version)", () => {
    setMockState({ error: "error sending request for url (https://github.com/...)" });
    const { container } = render(() => <UpdateProgressDialog />);
    const h4 = container.querySelector("h4");
    expect(h4?.textContent).toBe("Update failed");
  });

  it("shows 'Update failed' even when version is also set", () => {
    setMockState({ error: "network error", version: "1.2.3" });
    const { container } = render(() => <UpdateProgressDialog />);
    expect(container.querySelector("h4")?.textContent).toBe("Update failed");
  });

  it("shows 'Updating to v<version>' while downloading", () => {
    setMockState({ downloading: true, version: "1.2.3" });
    const { container } = render(() => <UpdateProgressDialog />);
    expect(container.querySelector("h4")?.textContent).toBe("Updating to v1.2.3");
  });

  it("does not render the dialog when idle (no downloading, no error)", () => {
    const { container } = render(() => <UpdateProgressDialog />);
    expect(container.querySelector("h4")).toBeNull();
  });
});
