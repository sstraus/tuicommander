import { describe, it, expect, beforeEach, afterEach } from "vitest";

describe("panel mode URL parsing", () => {
  let originalSearch: string;

  beforeEach(() => {
    originalSearch = window.location.search;
  });

  afterEach(() => {
    Object.defineProperty(window, "location", {
      value: { ...window.location, search: originalSearch },
      writable: true,
    });
  });

  function setSearch(qs: string) {
    Object.defineProperty(window, "location", {
      value: { ...window.location, search: qs },
      writable: true,
    });
  }

  it("detects panel mode from query", () => {
    setSearch("?mode=panel&panel=ai-chat");
    const params = new URLSearchParams(window.location.search);
    expect(params.get("mode")).toBe("panel");
    expect(params.get("panel")).toBe("ai-chat");
  });

  it("default mode has no panel param", () => {
    setSearch("");
    const params = new URLSearchParams(window.location.search);
    expect(params.get("mode")).toBeNull();
  });

  it("reads chatId from query in panel mode", () => {
    setSearch("?mode=panel&panel=ai-chat&chatId=abc123");
    const params = new URLSearchParams(window.location.search);
    expect(params.get("chatId")).toBe("abc123");
  });

  it("chatId is null when not provided", () => {
    setSearch("?mode=panel&panel=ai-chat");
    const params = new URLSearchParams(window.location.search);
    expect(params.get("chatId")).toBeNull();
  });
});
