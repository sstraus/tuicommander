import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { render, fireEvent } from "@solidjs/testing-library";
import { SettingsShell } from "../../components/SettingsPanel/SettingsShell";

describe("SettingsShell", () => {
  const tabs = [
    { key: "general", label: "General" },
    { key: "appearance", label: "Appearance" },
  ];

  const defaultProps = {
    visible: true,
    onClose: vi.fn(),
    title: "Settings",
    tabs,
    activeTab: "general",
    onTabChange: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("does not render when visible=false", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps} visible={false}>
        <p>content</p>
      </SettingsShell>
    ));
    expect(container.querySelector(".settings-overlay")).toBeNull();
  });

  it("renders overlay and panel when visible", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    expect(container.querySelector(".settings-overlay")).not.toBeNull();
    expect(container.querySelector(".settings-panel")).not.toBeNull();
  });

  it("renders title in header", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    const h2 = container.querySelector(".settings-header h2");
    expect(h2).not.toBeNull();
    expect(h2!.textContent).toBe("Settings");
  });

  it("renders subtitle when provided", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps} subtitle="/path/to/repo">
        <p>content</p>
      </SettingsShell>
    ));
    const sub = container.querySelector(".settings-path--repo");
    expect(sub).not.toBeNull();
    expect(sub!.textContent).toBe("/path/to/repo");
  });

  it("renders icon when provided", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps} icon="📁">
        <p>content</p>
      </SettingsShell>
    ));
    const icon = container.querySelector(".settings-icon--repo");
    expect(icon).not.toBeNull();
    expect(icon!.textContent).toBe("📁");
  });

  it("renders tab buttons", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    const tabBtns = container.querySelectorAll(".settings-tab");
    expect(tabBtns.length).toBe(2);
    expect(tabBtns[0].textContent).toBe("General");
    expect(tabBtns[1].textContent).toBe("Appearance");
  });

  it("marks active tab", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    const tabBtns = container.querySelectorAll(".settings-tab");
    expect(tabBtns[0].classList.contains("active")).toBe(true);
    expect(tabBtns[1].classList.contains("active")).toBe(false);
  });

  it("clicking tab calls onTabChange", () => {
    const onTabChange = vi.fn();
    const { container } = render(() => (
      <SettingsShell {...defaultProps} onTabChange={onTabChange}>
        <p>content</p>
      </SettingsShell>
    ));
    const tabBtns = container.querySelectorAll(".settings-tab");
    fireEvent.click(tabBtns[1]);
    expect(onTabChange).toHaveBeenCalledWith("appearance");
  });

  it("close button calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsShell {...defaultProps} onClose={onClose}>
        <p>content</p>
      </SettingsShell>
    ));
    fireEvent.click(container.querySelector(".settings-close")!);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("overlay click calls onClose", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <SettingsShell {...defaultProps} onClose={onClose}>
        <p>content</p>
      </SettingsShell>
    ));
    fireEvent.click(container.querySelector(".settings-overlay")!);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("renders children in content area", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p class="test-child">hello</p>
      </SettingsShell>
    ));
    const child = container.querySelector(".settings-content .test-child");
    expect(child).not.toBeNull();
    expect(child!.textContent).toBe("hello");
  });

  it("renders footer when provided", () => {
    const footer = <div class="test-footer">footer content</div>;
    const { container } = render(() => (
      <SettingsShell {...defaultProps} footer={footer}>
        <p>content</p>
      </SettingsShell>
    ));
    const footerEl = container.querySelector(".test-footer");
    expect(footerEl).not.toBeNull();
    expect(footerEl!.textContent).toBe("footer content");
  });

  it("applies header--repo modifier when icon or subtitle present", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps} icon="📁" subtitle="/path">
        <p>content</p>
      </SettingsShell>
    ));
    const header = container.querySelector(".settings-header");
    expect(header!.classList.contains("settings-header--repo")).toBe(true);
  });
});
