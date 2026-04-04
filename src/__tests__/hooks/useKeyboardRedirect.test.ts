import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import "../mocks/tauri";

// Use vi.hoisted so these are available when the mock factory runs (vi.mock is hoisted)
const { mockWrite, mockFocus, mockGetActive } = vi.hoisted(() => ({
  mockWrite: vi.fn(),
  mockFocus: vi.fn(),
  mockGetActive: vi.fn(),
}));

vi.mock("../../stores/terminals", () => ({
  terminalsStore: {
    getActive: mockGetActive,
  },
}));

import { createRoot } from "solid-js";
import { useKeyboardRedirect } from "../../hooks/useKeyboardRedirect";
import { testInScopeAsync } from "../helpers/store";

/** Dispatch a keydown event on document */
function dispatchKeydown(key: string, opts: Partial<KeyboardEvent> = {}): void {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...opts,
  });
  document.dispatchEvent(event);
}

/** Flush SolidJS effects (createEffect uses queueMicrotask internally) */
function flushEffects(): Promise<void> {
  return new Promise((resolve) => queueMicrotask(resolve));
}

describe("useKeyboardRedirect", () => {
  beforeEach(() => {
    mockWrite.mockReset();
    mockFocus.mockReset();
    mockGetActive.mockReset();
    // Default: active terminal exists with ref
    mockGetActive.mockReturnValue({
      id: "term-1",
      ref: {
        write: mockWrite,
        focus: mockFocus,
        fit: vi.fn(),
        writeln: vi.fn(),
        clear: vi.fn(),
        getSessionId: vi.fn(),
        openSearch: vi.fn(),
        closeSearch: vi.fn(),
        searchBuffer: vi.fn(() => []),
        scrollToLine: vi.fn(),
        scrollToTop: vi.fn(),
        scrollToBottom: vi.fn(),
        scrollPages: vi.fn(),
      },
    });
  });

  afterEach(() => {
    // Ensure no DOM state leaks between tests
    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  });

  describe("printable character redirect", () => {
    it("redirects a printable character to the active terminal", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("a");

        expect(mockWrite).toHaveBeenCalledWith("a");
        expect(mockFocus).toHaveBeenCalled();

      });
    });

    it("redirects uppercase letters", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("Z");

        expect(mockWrite).toHaveBeenCalledWith("Z");

      });
    });

    it("redirects space character", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown(" ");

        expect(mockWrite).toHaveBeenCalledWith(" ");

      });
    });
  });

  describe("special keys", () => {
    it("redirects Backspace as DEL character", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("Backspace");

        expect(mockWrite).toHaveBeenCalledWith("\x7f");

      });
    });

    it("redirects Delete as escape sequence", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("Delete");

        expect(mockWrite).toHaveBeenCalledWith("\x1b[3~");

      });
    });
  });

  describe("excluded keys", () => {
    it("does not redirect Tab", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("Tab");

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });

    it("does not redirect Escape", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("Escape");

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });

    it("does not redirect arrow keys", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("ArrowUp");
        dispatchKeydown("ArrowDown");
        dispatchKeydown("ArrowLeft");
        dispatchKeydown("ArrowRight");

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });

    it("does not redirect function keys", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("F1");
        dispatchKeydown("F12");

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });

    it("does not redirect Enter", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("Enter");

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });
  });

  describe("modifier keys", () => {
    it("does not redirect when Ctrl is held", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("c", { ctrlKey: true });

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });

    it("does not redirect when Meta/Cmd is held", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("v", { metaKey: true });

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });

    it("does not redirect when Alt is held", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("x", { altKey: true });

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });
  });

  describe("focus context", () => {
    it("does not redirect when focus is on an input element", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        const input = document.createElement("input");
        document.body.appendChild(input);
        input.focus();

        dispatchKeydown("a");

        expect(mockWrite).not.toHaveBeenCalled();

        document.body.removeChild(input);
      });
    });

    it("does not redirect when focus is on a textarea", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        const textarea = document.createElement("textarea");
        document.body.appendChild(textarea);
        textarea.focus();

        dispatchKeydown("b");

        expect(mockWrite).not.toHaveBeenCalled();

        document.body.removeChild(textarea);
      });
    });

    it("does not redirect when focus is on a select element", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        const select = document.createElement("select");
        document.body.appendChild(select);
        select.focus();

        dispatchKeydown("c");

        expect(mockWrite).not.toHaveBeenCalled();

        document.body.removeChild(select);
      });
    });

    it("does not redirect when focus is inside a terminal pane", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        const terminalPane = document.createElement("div");
        terminalPane.classList.add("terminal-pane");
        const child = document.createElement("div");
        child.setAttribute("tabindex", "0");
        terminalPane.appendChild(child);
        document.body.appendChild(terminalPane);
        child.focus();

        dispatchKeydown("d");

        expect(mockWrite).not.toHaveBeenCalled();

        document.body.removeChild(terminalPane);
      });
    });

    it("does not redirect when focus is inside an xterm element", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect();
        await flushEffects();

        const xterm = document.createElement("div");
        xterm.classList.add("xterm");
        const child = document.createElement("div");
        child.setAttribute("tabindex", "0");
        xterm.appendChild(child);
        document.body.appendChild(xterm);
        child.focus();

        dispatchKeydown("e");

        expect(mockWrite).not.toHaveBeenCalled();

        document.body.removeChild(xterm);
      });
    });
  });

  describe("no active terminal", () => {
    it("does not write when there is no active terminal", async () => {
      await testInScopeAsync(async () => {
        mockGetActive.mockReturnValue(undefined);
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("a");

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });

    it("does not write when active terminal has no ref", async () => {
      await testInScopeAsync(async () => {
        mockGetActive.mockReturnValue({ id: "term-1", ref: undefined });
        useKeyboardRedirect();
        await flushEffects();

        dispatchKeydown("a");

        expect(mockWrite).not.toHaveBeenCalled();

      });
    });
  });

  describe("autoFocus parameter", () => {
    it("does not focus terminal when autoFocus is false", async () => {
      await testInScopeAsync(async () => {
        useKeyboardRedirect(false);
        await flushEffects();

        dispatchKeydown("a");

        expect(mockWrite).toHaveBeenCalledWith("a");
        expect(mockFocus).not.toHaveBeenCalled();

      });
    });
  });

  describe("cleanup", () => {
    it("removes event listener on dispose", async () => {
      await createRoot(async (dispose) => {
        useKeyboardRedirect();
        await flushEffects();

        // Verify it works before dispose
        dispatchKeydown("a");
        expect(mockWrite).toHaveBeenCalledTimes(1);

        dispose();

        // After dispose, should not redirect
        mockWrite.mockReset();
        dispatchKeydown("b");
        expect(mockWrite).not.toHaveBeenCalled();
      });
    });
  });
});
