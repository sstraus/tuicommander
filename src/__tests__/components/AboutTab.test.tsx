import { describe, it, expect, vi } from "vitest";
import { render } from "@solidjs/testing-library";
import { AboutTab } from "../../components/SettingsPanel/tabs/AboutTab";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

describe("AboutTab", () => {
  it("renders the heading", () => {
    const { container } = render(() => <AboutTab />);
    const heading = container.querySelector("h3");
    expect(heading).not.toBeNull();
    expect(heading!.textContent).toBe("About TUICommander");
  });

  it("displays the version", () => {
    const { container } = render(() => <AboutTab />);
    expect(container.textContent).toContain("0.3.0");
  });

  it("displays the description", () => {
    const { container } = render(() => <AboutTab />);
    expect(container.textContent).toContain("terminal multiplexer");
  });

  it("has link buttons", () => {
    const { container } = render(() => <AboutTab />);
    const buttons = container.querySelectorAll("button");
    const buttonTexts = Array.from(buttons).map((b) => b.textContent?.trim());
    expect(buttonTexts).toContain("GitHub Repository");
    expect(buttonTexts).toContain("Documentation");
    expect(buttonTexts).toContain("Report an Issue");
  });

  it("displays license and credits", () => {
    const { container } = render(() => <AboutTab />);
    expect(container.textContent).toContain("MIT License");
    expect(container.textContent).toContain("Tauri 2");
    expect(container.textContent).toContain("SolidJS");
  });
});
