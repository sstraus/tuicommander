import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@solidjs/testing-library";

const { mockDetachPanel, mockReattachPanel, mockClosePanel } = vi.hoisted(() => ({
  mockDetachPanel: vi.fn().mockResolvedValue(undefined),
  mockReattachPanel: vi.fn().mockResolvedValue(undefined),
  mockClosePanel: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../panelRouter", () => ({
  detachPanel: mockDetachPanel,
  reattachPanel: mockReattachPanel,
  closePanel: mockClosePanel,
}));

vi.mock("../../transport", () => ({
  isTauri: () => true,
}));

import { PanelWindowControls } from "../../components/ui/PanelWindowControls";

describe("PanelWindowControls", () => {
  beforeEach(() => {
    mockDetachPanel.mockReset().mockResolvedValue(undefined);
    mockReattachPanel.mockReset().mockResolvedValue(undefined);
    mockClosePanel.mockReset().mockResolvedValue(undefined);
  });

  describe("inline mode", () => {
    it("renders detach button", () => {
      const { container } = render(() => (
        <PanelWindowControls panelId="activity" mode="inline" />
      ));
      const detachBtn = container.querySelector("[title='Open in separate window']");
      expect(detachBtn).toBeTruthy();
    });

    it("does not render reattach button", () => {
      const { container } = render(() => (
        <PanelWindowControls panelId="activity" mode="inline" />
      ));
      const reattachBtn = container.querySelector("[title='Bring back to main window']");
      expect(reattachBtn).toBeNull();
    });

    it("renders close button that calls onInlineClose", () => {
      const onClose = vi.fn();
      const { container } = render(() => (
        <PanelWindowControls panelId="activity" mode="inline" onInlineClose={onClose} />
      ));
      const closeBtn = container.querySelector("[title='Close']");
      expect(closeBtn).toBeTruthy();
      (closeBtn as HTMLElement).click();
      expect(onClose).toHaveBeenCalledOnce();
    });

    it("click detach calls detachPanel with panelId", () => {
      const { container } = render(() => (
        <PanelWindowControls panelId="test-panel" mode="inline" />
      ));
      const detachBtn = container.querySelector("[title='Open in separate window']") as HTMLElement;
      detachBtn.click();
      expect(mockDetachPanel).toHaveBeenCalledWith("test-panel");
    });
  });

  describe("detached mode", () => {
    it("renders reattach button", () => {
      const { container } = render(() => (
        <PanelWindowControls panelId="activity" mode="detached" />
      ));
      const reattachBtn = container.querySelector("[title='Bring back to main window']");
      expect(reattachBtn).toBeTruthy();
    });

    it("does not render detach button", () => {
      const { container } = render(() => (
        <PanelWindowControls panelId="activity" mode="detached" />
      ));
      const detachBtn = container.querySelector("[title='Open in separate window']");
      expect(detachBtn).toBeNull();
    });

    it("renders close button that calls closePanel", () => {
      const { container } = render(() => (
        <PanelWindowControls panelId="test-panel" mode="detached" />
      ));
      const closeBtn = container.querySelector("[title='Close']") as HTMLElement;
      closeBtn.click();
      expect(mockClosePanel).toHaveBeenCalledWith("test-panel");
    });

    it("click reattach calls reattachPanel with panelId", () => {
      const { container } = render(() => (
        <PanelWindowControls panelId="test-panel" mode="detached" />
      ));
      const reattachBtn = container.querySelector("[title='Bring back to main window']") as HTMLElement;
      reattachBtn.click();
      expect(mockReattachPanel).toHaveBeenCalledWith("test-panel");
    });
  });
});
