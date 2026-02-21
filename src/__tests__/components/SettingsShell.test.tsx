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

  it("renders nav items in sidebar", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    const navItems = container.querySelectorAll(".settings-nav-item");
    expect(navItems.length).toBe(2);
    expect(navItems[0].textContent).toBe("General");
    expect(navItems[1].textContent).toBe("Appearance");
  });

  it("marks active nav item", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    const navItems = container.querySelectorAll(".settings-nav-item");
    expect(navItems[0].classList.contains("active")).toBe(true);
    expect(navItems[1].classList.contains("active")).toBe(false);
  });

  it("clicking nav item calls onTabChange", () => {
    const onTabChange = vi.fn();
    const { container } = render(() => (
      <SettingsShell {...defaultProps} onTabChange={onTabChange}>
        <p>content</p>
      </SettingsShell>
    ));
    const navItems = container.querySelectorAll(".settings-nav-item");
    fireEvent.click(navItems[1]);
    expect(onTabChange).toHaveBeenCalledWith("appearance");
  });

  it("renders section label for __label__: keys", () => {
    const tabsWithLabel = [
      { key: "a", label: "A" },
      { key: "__sep__", label: "─" },
      { key: "__label__:Repositories", label: "REPOSITORIES" },
      { key: "b", label: "B" },
    ];
    const { container } = render(() => (
      <SettingsShell {...defaultProps} tabs={tabsWithLabel}>
        <p>content</p>
      </SettingsShell>
    ));
    const label = container.querySelector(".settings-nav-label");
    expect(label).not.toBeNull();
    expect(label!.textContent).toBe("REPOSITORIES");
    // Label is not a button (not clickable)
    expect(label!.tagName.toLowerCase()).not.toBe("button");
    // Only actual nav items are buttons, not the label
    const navItems = container.querySelectorAll(".settings-nav-item");
    expect(navItems.length).toBe(2); // a and b, not the label
  });

  it("repo: prefixed nav items get --repo modifier class", () => {
    const tabsWithRepo = [
      { key: "general", label: "General" },
      { key: "repo:/path/to/repo", label: "my-repo" },
    ];
    const { container } = render(() => (
      <SettingsShell {...defaultProps} tabs={tabsWithRepo} activeTab="general">
        <p>content</p>
      </SettingsShell>
    ));
    const repoItem = container.querySelector(".settings-nav-item--repo");
    expect(repoItem).not.toBeNull();
    expect(repoItem!.textContent).toBe("my-repo");
  });

  it("renders separator between nav groups", () => {
    const tabsWithSep = [
      { key: "a", label: "A" },
      { key: "__sep__", label: "─" },
      { key: "b", label: "B" },
    ];
    const { container } = render(() => (
      <SettingsShell {...defaultProps} tabs={tabsWithSep}>
        <p>content</p>
      </SettingsShell>
    ));
    const navItems = container.querySelectorAll(".settings-nav-item");
    const separator = container.querySelector(".settings-nav-separator");
    expect(navItems.length).toBe(2);
    expect(separator).not.toBeNull();
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

  it("nav sidebar renders with default width when navWidth not provided", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    const nav = container.querySelector(".settings-nav") as HTMLElement | null;
    expect(nav).not.toBeNull();
  });

  it("renders split layout with nav and content side by side", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    const body = container.querySelector(".settings-body");
    expect(body).not.toBeNull();
    expect(body!.querySelector(".settings-nav")).not.toBeNull();
    expect(body!.querySelector(".settings-content")).not.toBeNull();
  });

  it("renders resize handle inside nav", () => {
    const { container } = render(() => (
      <SettingsShell {...defaultProps}>
        <p>content</p>
      </SettingsShell>
    ));
    expect(container.querySelector(".settings-nav-resize-handle")).not.toBeNull();
  });
});
