import { describe, it, expect } from "vitest";
import { TUIC_SDK_SCRIPT, TUIC_SDK_VERSION } from "../tuicSdk";

describe("TUIC SDK Script", () => {
  it("exports a non-empty script string", () => {
    expect(TUIC_SDK_SCRIPT).toBeTruthy();
    expect(typeof TUIC_SDK_SCRIPT).toBe("string");
  });

  it("wraps content in a script tag", () => {
    expect(TUIC_SDK_SCRIPT).toMatch(/^<script id="tuic-sdk">/);
    expect(TUIC_SDK_SCRIPT).toMatch(/<\/script>$/);
  });

  it("assigns window.tuic with version", () => {
    expect(TUIC_SDK_SCRIPT).toContain("window.tuic");
    expect(TUIC_SDK_SCRIPT).toContain(`version:"${TUIC_SDK_VERSION}"`);
  });

  it("defines open method that sends tuic:open message", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:open");
    expect(TUIC_SDK_SCRIPT).toContain("parent.postMessage");
  });

  it("defines edit method that sends tuic:edit message", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:edit");
  });

  it("defines terminal method that sends tuic:terminal message", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:terminal");
  });

  it("includes delegated click listener for tuic:// links", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic://");
    expect(TUIC_SDK_SCRIPT).toContain("addEventListener");
  });

  it("reads data-pinned attribute from links", () => {
    expect(TUIC_SDK_SCRIPT).toContain("data-pinned");
  });

  it("defines activeRepo method", () => {
    expect(TUIC_SDK_SCRIPT).toContain("activeRepo");
  });

  it("listens for tuic:repo-changed messages from parent", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:repo-changed");
  });

  it("defines onRepoChange and offRepoChange for listener management", () => {
    expect(TUIC_SDK_SCRIPT).toContain("onRepoChange");
    expect(TUIC_SDK_SCRIPT).toContain("offRepoChange");
  });

  it("defines toast method that sends tuic:toast message with sound option", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:toast");
    expect(TUIC_SDK_SCRIPT).toContain("sound");
  });

  it("defines clipboard method that sends tuic:clipboard message", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:clipboard");
  });

  it("defines getFile method that sends tuic:get-file message", () => {
    expect(TUIC_SDK_SCRIPT).toContain("getFile");
    expect(TUIC_SDK_SCRIPT).toContain("tuic:get-file");
  });

  it("handles tuic:get-file-result responses from host", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:get-file-result");
  });

  it("defines onMessage method for host-to-plugin messaging", () => {
    expect(TUIC_SDK_SCRIPT).toContain("onMessage");
  });

  it("defines send method that sends tuic:plugin-message", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:plugin-message");
  });

  it("handles tuic:host-message from host", () => {
    expect(TUIC_SDK_SCRIPT).toContain("tuic:host-message");
  });

  it("defines theme property and onThemeChange listener", () => {
    expect(TUIC_SDK_SCRIPT).toContain("onThemeChange");
    expect(TUIC_SDK_SCRIPT).toContain("tuic:theme-changed");
  });
});
