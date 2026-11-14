import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { resolve, join } from "node:path";
import { promisify } from "node:util";
import { test } from "node:test";
import { createServer } from "vite";
import solid from "vite-plugin-solid";

// Real line boxes are required: DOM emulators cannot detect clipped paths.
// Catches: a long updater path or a multiline cause requires horizontal scrolling.
test("update errors remain fully readable inside the dialog", { timeout: 240000 }, async () => {
  const directory = resolve(process.env.TUIC_UPDATE_ERROR_EVIDENCE ?? ".tmp/update-error");
  await mkdir(directory, { recursive: true });
  const wrapper = join(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
  const run = promisify(execFile);
  const browser = async (...args) => (await run(wrapper, args, {
    env: { ...process.env, AB_SESSION: "tuic2724" }, timeout: 40000, maxBuffer: 2 ** 20,
  })).stdout;
  const server = await createServer({ configFile: false, plugins: [solid()],
    cacheDir: ".tmp/update-error-vite", logLevel: "error",
    define: { __APP_VERSION__: JSON.stringify("2.0.0"), __BUILD_GIT_HASH__: JSON.stringify("geometry") },
    server: { host: "127.0.0.1", port: 0 },
  });
  const measurements = [];
  try {
    await server.listen();
    for (const width of [390, 1200]) {
      for (const scenario of [0, 1, 2]) {
        await browser(`http://127.0.0.1:${server.httpServer.address().port}/src/__tests__/browser/updateErrorGeometry.html?case=${scenario}`);
        await browser("--", "set", "viewport", String(width), "800");
        const output = await browser("--", "eval", `(() => {
          if (!window.updateErrorReady) throw new Error("Dialog fixture did not load");
          const error = document.querySelector("p");
          const body = error.parentElement;
          const dialog = body.parentElement;
          const range = document.createRange(); range.selectNodeContents(error);
          const box = error.getBoundingClientRect();
          return {width: innerWidth, scenario: ${scenario}, text: error.textContent,
            expected: window.updateErrorExpected, scrollWidth: body.scrollWidth, clientWidth: body.clientWidth,
            lineCount: range.getClientRects().length, box: {left: box.left, right: box.right},
            lines: [...range.getClientRects()].map(r => ({left:r.left, right:r.right})),
            closeVisible: [...dialog.querySelectorAll("button")].some(b => b.textContent.trim() === "Close" && b.getBoundingClientRect().bottom <= innerHeight)};
        })()`);
        const sample = JSON.parse(output);
        measurements.push(sample);
        await browser("--", "screenshot", join(directory, `error-${width}-${scenario}.png`));
      }
    }
    await writeFile(join(directory, "measurements.json"), JSON.stringify(measurements, null, 2));
    assert.equal(measurements.length, 6);
    for (const sample of measurements) {
      assert.equal(sample.text, sample.expected);
      assert.ok(sample.closeVisible, "Close stays reachable");
      assert.ok(sample.scrollWidth <= sample.clientWidth + 1, JSON.stringify(sample));
      for (const line of sample.lines) assert.ok(line.left >= sample.box.left - 1 && line.right <= sample.box.right + 1, JSON.stringify(sample));
      if (sample.scenario < 2) assert.ok(sample.lineCount > 1, "Long paths wrap into multiple visible lines");
    }
  } finally {
    await browser("--", "tab", "close").catch(() => {});
    await server.close();
  }
});
