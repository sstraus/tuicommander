import { describe, it, expect } from "vitest";
import "../mocks/tauri";

describe("branchSwitcherStore", () => {
  // Dynamic import to get fresh store per test file
  async function freshStore() {
    const mod = await import("../../stores/branchSwitcher");
    return mod.branchSwitcherStore;
  }

  it("starts closed with empty query", async () => {
    const store = await freshStore();
    expect(store.state.isOpen).toBe(false);
    expect(store.state.query).toBe("");
  });

  it("open() sets isOpen true and clears query", async () => {
    const store = await freshStore();
    store.setQuery("stale");
    store.open();
    expect(store.state.isOpen).toBe(true);
    expect(store.state.query).toBe("");
  });

  it("close() sets isOpen false and clears query", async () => {
    const store = await freshStore();
    store.open();
    store.setQuery("test");
    store.close();
    expect(store.state.isOpen).toBe(false);
    expect(store.state.query).toBe("");
  });

  it("toggle() opens when closed and closes when open", async () => {
    const store = await freshStore();
    expect(store.state.isOpen).toBe(false);
    store.toggle();
    expect(store.state.isOpen).toBe(true);
    store.toggle();
    expect(store.state.isOpen).toBe(false);
  });

  it("setQuery() updates query", async () => {
    const store = await freshStore();
    store.setQuery("feat/");
    expect(store.state.query).toBe("feat/");
  });
});
