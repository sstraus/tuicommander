import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

const root = new URL("../../../", import.meta.url);
const source = readFileSync(new URL("website/main.js", root), "utf8");
const release = JSON.parse(readFileSync(new URL("src-tauri/test-fixtures/updater/github-stable-release.json", root), "utf8"));

// Catches: Linux ARM64/unknown browsers receive x64 assets; macOS/Windows links regress.
for (const [platform, suffix] of [["Linux aarch64", null], ["Linux x86_64", null], ["MacIntel", ".dmg"], ["Win32", "x64-setup.exe"]]) {
  test(`download selection for ${platform}`, async () => {
    const chooser = "https://github.com/sstraus/tuicommander/releases";
    const button = { dataset: { download: "auto" }, href: chooser };
    const document = {
      body: { classList: { add() {} } },
      querySelectorAll(selector) { return selector === "[data-download]" ? [button] : []; },
      querySelector() { return null; },
      getElementById() { return null; },
      addEventListener() {},
    };
    vm.runInNewContext(source, {
      document,
      window: {},
      location: { pathname: "/download.html" },
      navigator: { platform, userAgent: "" },
      fetch: async () => ({ json: async () => [release] }),
    });
    await new Promise((resolve) => setImmediate(resolve));
    const expected = suffix ? release.assets.find((asset) => asset.name.endsWith(suffix)).browser_download_url : chooser;
    assert.equal(button.href, expected);
  });
}
