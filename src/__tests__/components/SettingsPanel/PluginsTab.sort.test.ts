import { describe, it, expect } from "vitest";
import { sortPlugins } from "../../../components/SettingsPanel/tabs/PluginsTab";
import type { PluginState } from "../../../stores/pluginStore";
import { PluginLogger } from "../../../plugins/pluginLogger";

function make(id: string, opts: Partial<PluginState> = {}): PluginState {
  return {
    id,
    manifest: { name: opts.manifest?.name ?? id, version: "1.0.0" } as PluginState["manifest"],
    builtIn: false,
    enabled: true,
    loaded: true,
    paused: false,
    error: null,
    logger: new PluginLogger(),
    ...opts,
  };
}

describe("sortPlugins", () => {
  it("places built-in plugins before external ones", () => {
    const plugins = [
      make("ext-a", { builtIn: false }),
      make("builtin-z", { builtIn: true }),
      make("ext-b", { builtIn: false }),
      make("builtin-a", { builtIn: true }),
    ];
    const sorted = sortPlugins(plugins).map((p) => p.id);
    expect(sorted.slice(0, 2).sort()).toEqual(["builtin-a", "builtin-z"]);
    expect(sorted.slice(2).sort()).toEqual(["ext-a", "ext-b"]);
  });

  it("within built-in, enabled come before disabled", () => {
    const plugins = [
      make("b-disabled", { builtIn: true, enabled: false }),
      make("b-enabled", { builtIn: true, enabled: true }),
    ];
    expect(sortPlugins(plugins).map((p) => p.id)).toEqual(["b-enabled", "b-disabled"]);
  });

  it("within external, enabled come before disabled", () => {
    const plugins = [
      make("e-disabled", { builtIn: false, enabled: false }),
      make("e-enabled", { builtIn: false, enabled: true }),
    ];
    expect(sortPlugins(plugins).map((p) => p.id)).toEqual(["e-enabled", "e-disabled"]);
  });

  it("sorts alphabetically by display name within the same bucket", () => {
    const plugins = [
      make("c", { manifest: { name: "Charlie", version: "1" } as PluginState["manifest"] }),
      make("a", { manifest: { name: "alpha", version: "1" } as PluginState["manifest"] }),
      make("b", { manifest: { name: "Bravo", version: "1" } as PluginState["manifest"] }),
    ];
    expect(sortPlugins(plugins).map((p) => p.id)).toEqual(["a", "b", "c"]);
  });

  it("falls back to id when manifest.name is missing", () => {
    const plugins = [
      make("zebra", { manifest: null }),
      make("apple", { manifest: null }),
    ];
    expect(sortPlugins(plugins).map((p) => p.id)).toEqual(["apple", "zebra"]);
  });

  it("priority: builtIn > enabled > name", () => {
    const plugins = [
      make("ext-enabled-a", { builtIn: false, enabled: true, manifest: { name: "a", version: "1" } as PluginState["manifest"] }),
      make("builtin-disabled-z", { builtIn: true, enabled: false, manifest: { name: "z", version: "1" } as PluginState["manifest"] }),
      make("builtin-enabled-m", { builtIn: true, enabled: true, manifest: { name: "m", version: "1" } as PluginState["manifest"] }),
      make("ext-disabled-b", { builtIn: false, enabled: false, manifest: { name: "b", version: "1" } as PluginState["manifest"] }),
    ];
    expect(sortPlugins(plugins).map((p) => p.id)).toEqual([
      "builtin-enabled-m",
      "builtin-disabled-z",
      "ext-enabled-a",
      "ext-disabled-b",
    ]);
  });

  it("does not mutate the input array", () => {
    const plugins = [
      make("b", { builtIn: false }),
      make("a", { builtIn: true }),
    ];
    const original = [...plugins];
    sortPlugins(plugins);
    expect(plugins).toEqual(original);
  });
});
