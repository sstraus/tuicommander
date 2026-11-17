/** Real component rectangles catch a jump control covering keys or entering the composer. */
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { createServer } from "vite";
import solid from "vite-plugin-solid";
import { testTmpRoot } from "./test-tmp-root.mjs";

// Default evidence lives under the caller's temp dir, never inside the checkout.
const evidence = resolve(process.env.MOBILE_JUMP_EVIDENCE ?? join(testTmpRoot(), "mobile-jump-layout"));
mkdirSync(evidence, { recursive: true });
const browsers = join(process.env.HOME, ".agent-browser/browsers");
const chrome = process.env.CHROME_BIN ?? join(browsers, readdirSync(browsers).filter(n => n.startsWith("chrome-")).sort().at(-1), "Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing");
const server = await createServer({ configFile: false, plugins: [solid()], define: { __APP_VERSION__: '"layout"', __BUILD_GIT_HASH__: '"layout"' }, server: { host: "127.0.0.1", port: 19927, strictPort: true } });
let browser;
let ws;
try {
 await server.listen();
 browser = spawn(chrome, ["--headless=new", "--no-first-run", "--no-default-browser-check", `--user-data-dir=${join(evidence, "chrome")}`, "--remote-debugging-port=19928", "about:blank"], { stdio: "ignore" });
 let tabs;
 for (let i = 0; i < 100; i++) {
  try { tabs = await (await fetch("http://127.0.0.1:19928/json")).json(); if (tabs.some(t => t.type === "page")) break; } catch {}
  await delay(100);
 }
 ws = new WebSocket(tabs.find(t => t.type === "page").webSocketDebuggerUrl);
 await new Promise(resolve => { ws.onopen = resolve; });
 let id = 0;
 const pending = new Map();
 ws.onmessage = event => { const m = JSON.parse(event.data); if (m.id) { const p = pending.get(m.id); pending.delete(m.id); if (m.error) p.reject(new Error(JSON.stringify(m.error))); else p.resolve(m.result); } };
 const call = (method, params = {}) => new Promise((resolve, reject) => { const next = ++id; pending.set(next, { resolve, reject }); ws.send(JSON.stringify({ id: next, method, params })); });
 await call("Page.enable");
 const results = [];
 for (const [width, height] of [[360,800],[390,844],[844,390]]) {
  await call("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: true });
  await call("Page.navigate", { url: "http://127.0.0.1:19927/scripts/fixtures/mobile-jump-layout.html" });
  let measurement;
  for (let i = 0; i < 100; i++) {
   const value = await call("Runtime.evaluate", { expression: `(() => {
    const jump = document.querySelector('[aria-label="Scroll to bottom"]');
    const send = document.querySelector('[aria-label="Send"]');
    const key = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === '←');
    if (!jump || !send || !key) return null;
    const rect = e => { const r=e.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom}; };
    return { jump:rect(jump),send:rect(send),keybar:rect(key.parentElement),output:rect(document.querySelector("[data-output-area]")),composer:rect(send.parentElement),keys:[...key.parentElement.querySelectorAll('button')].map(rect) };
   })()`, returnByValue: true });
   measurement = value.result.value;
   if (measurement) break;
   await delay(100);
  }
  assert.ok(measurement, "real components must mount");
  results.push({ width, height, ...measurement });
  // Scroll the existing horizontal key bar to expose all arrow keys in the image.
  await call("Runtime.evaluate", { expression: `(() => { const key=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='←'); key.parentElement.scrollLeft=key.parentElement.scrollWidth; })()` });
  writeFileSync(join(evidence, `${width}.png`), Buffer.from((await call("Page.captureScreenshot", {format:"png"})).data, "base64"));
 }
 writeFileSync(join(evidence,"measurements.json"), JSON.stringify(results,null,2));
 if (!process.argv.includes("--capture-only")) {
  for (const {width,jump,keybar,output,composer,keys} of results) {
   assert.ok(keys.every(k => jump.right <= k.left || jump.left >= k.right || jump.bottom <= k.top || jump.top >= k.bottom), `${width}: jump overlaps a key-bar button`);
   assert.ok(keybar.top - jump.bottom >= 8, `${width}: jump bottom must clear key bar by at least 8px`);
   assert.ok(jump.bottom < composer.top, `${width}: jump must not touch composer row`);
   assert.ok(jump.top >= output.top && jump.bottom <= output.bottom && jump.left >= output.left && jump.right <= output.right, `${width}: jump must float within output area`);
   assert.ok(jump.right <= width && jump.left >= width - 64, `${width}: jump stays at bottom-right within viewport`);
  }
  console.log("PASS: output-area placement with 8px key-bar gap at 360, 390 and landscape 844");
 }
} finally {
 ws?.close();
 if (browser) { browser.kill("SIGTERM"); await new Promise(resolve => { if (browser.exitCode !== null) resolve(); else browser.once("exit",resolve); }); }
 await server.close();
}
