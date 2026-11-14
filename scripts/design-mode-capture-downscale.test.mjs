import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { test } from "node:test";

// Run: TUIC_PANEL_CHROME=<Chrome for Testing> scripts/with-test-tmp.sh node --test scripts/design-mode-capture-downscale.test.mjs
// Real Chrome encodes a worst-case 4K viewport (random pixels) through the shipped encoder.
// Catches: a real WebP/PNG encoding that never fits the transport bound, or downscaling that
// exceeds the 8-second browser command deadline.
const root = new URL("../browser-extension/design-mode/", import.meta.url);
const page = `<!doctype html><script type="module">
import {encodeCaptureWithin, MAX_SCREENSHOT_BYTES} from '/captureEncoding.js';
const canvas = new OffscreenCanvas(3840, 2160);
const context = canvas.getContext('2d');
const pixels = context.createImageData(3840, 2160);
for (let i = 0; i < pixels.data.length; i += 65536) crypto.getRandomValues(pixels.data.subarray(i, i + 65536));
for (let i = 3; i < pixels.data.length; i += 4) pixels.data[i] = 255;
context.putImageData(pixels, 0, 0);
window.run = async () => {
const results = [];
for (const args of [{format:'png'}, {format:'webp', quality:25}, {format:'webp', quality:90}]) {
  const started = performance.now();
  const bitmap = await createImageBitmap(canvas);
  const capture = await encodeCaptureWithin(bitmap, args, MAX_SCREENSHOT_BYTES);
  results.push({...args, bytes: capture.bytes, width: capture.width, height: capture.height,
    downscaled: capture.downscaled, ms: Math.round(performance.now() - started), limit: MAX_SCREENSHOT_BYTES});
}
return results;
};
</script>`;

test("real Chrome downscales a worst-case 4K capture under the transport bound", async () => {
  assert.ok(process.env.TUIC_PANEL_CHROME, "Set TUIC_PANEL_CHROME to Chrome for Testing");
  assert.ok(process.env.TUIC_TEST_TMP_ROOT, "Run through scripts/with-test-tmp.sh");
  const server = createServer(async (req, res) => {
    if (req.url === "/") { res.setHeader("Content-Type", "text/html"); res.end(page); return; }
    if (req.url === "/captureEncoding.js") {
      res.setHeader("Content-Type", "text/javascript");
      res.end(await readFile(new URL("captureEncoding.js", root)));
      return;
    }
    res.statusCode = 404; res.end();
  });
  const profile = await mkdtemp(join(process.env.TUIC_TEST_TMP_ROOT, "downscale-"));
  try {
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const browser = spawn(process.env.TUIC_PANEL_CHROME, [
      "--headless=new", "--no-first-run", "--no-default-browser-check", "--disable-background-networking",
      "--use-mock-keychain", "--password-store=basic", "--remote-debugging-pipe", `--user-data-dir=${profile}`,
      "about:blank",
    ], { stdio: ["ignore", "ignore", "ignore", "pipe", "pipe"] });
    let sequence = 0;
    let buffer = "";
    const pending = new Map();
    browser.stdio[4].on("data", (chunk) => {
      buffer += chunk;
      for (let end; (end = buffer.indexOf("\0")) !== -1; buffer = buffer.slice(end + 1)) {
        const message = JSON.parse(buffer.slice(0, end));
        pending.get(message.id)?.(message);
        pending.delete(message.id);
      }
    });
    const command = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
      const id = ++sequence;
      const deadline = setTimeout(() => { pending.delete(id); reject(new Error(`Chrome CDP timed out: ${method}`)); }, 90000);
      pending.set(id, (message) => { clearTimeout(deadline); if (message.error) reject(new Error(JSON.stringify(message.error))); else resolve(message.result); });
      browser.stdio[3].write(`${JSON.stringify({ id, method, params, sessionId })}\0`);
    });
    let results;
    try {
      const { targetId } = await command("Target.createTarget", { url: "about:blank" });
      const { sessionId } = await command("Target.attachToTarget", { targetId, flatten: true });
      await command("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/` }, sessionId);
      const reply = await command("Runtime.evaluate", { expression: "new Promise((resolve) => { const wait = () => window.run ? resolve(window.run()) : setTimeout(wait, 50); wait(); })", awaitPromise: true, returnByValue: true }, sessionId);
      assert.ok(!reply.exceptionDetails, JSON.stringify(reply.exceptionDetails));
      results = reply.result.value;
    } finally {
      const stopped = new Promise((resolve) => browser.once("exit", resolve));
      browser.kill("SIGTERM");
      await stopped;
    }
    assert.ok(Array.isArray(results), "encoder did not finish");
    console.log(JSON.stringify(results));
    for (const result of results) {
      assert.ok(result.bytes <= result.limit, `${result.format} ${result.bytes} bytes`);
      assert.equal(result.downscaled, true, `${result.format} should exceed the bound at 4K`);
      assert.ok(Math.abs(result.width / result.height - 16 / 9) < 0.01, `${result.width}x${result.height}`);
      assert.ok(result.ms < 8000, `${result.format} took ${result.ms} ms`);
    }
  } finally {
    server.close();
    await rm(profile, { recursive: true, force: true });
  }
});
