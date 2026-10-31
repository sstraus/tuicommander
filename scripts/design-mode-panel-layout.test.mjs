import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { test } from "node:test";

// Run: scripts/with-test-tmp.sh node --test scripts/design-mode-panel-layout.test.mjs
// Set TUIC_PANEL_CHROME to the bundled Chrome for Testing executable.
// --capture-only saves screenshots/geometry without running assertions.
const root = new URL("../browser-extension/design-mode/", import.meta.url);
// Component inputs only: Chrome performs the actual layout of the shipped page,
// stylesheet and panel.js. No browser-extension permission behavior is simulated.
const draft = {
  repoPath: "/workspace/review", connection: "Connected to TUICommander",
  tabId: 1, selectedSessionId: "reviewer", armedSessionId: "reviewer",
  agents: [{ sessionId: "reviewer", label: "Reviewer " + "long agent title ".repeat(10) }],
  picks: [{ id: "pick-1", comment: "", raw: { tagName: "button", selector: "#example", textContent: "Example" } }],
  servers: { servers: [{ url: "http://localhost:3000", source: "configured" }], canStart: true },
  errors: [{ kind: "error", message: "Example page error" }],
  actionHistory: [{ when: 0, who: "reviewer", action: "screenshot", ok: true }],
};
const widths = [280, 360, 520];

async function capture() {
  assert.ok(process.env.TUIC_PANEL_CHROME, "Set TUIC_PANEL_CHROME to Chrome for Testing");
  assert.ok(process.env.TUIC_TEST_TMP_ROOT, "Run through scripts/with-test-tmp.sh");
  const output = process.env.TUIC_PANEL_PROOF_DIR ?? process.env.TUIC_TEST_TMP_ROOT;
  await mkdir(output, { recursive: true });
  const html = await readFile(new URL("panel.html", root), "utf8");
  const stub = `<script>globalThis.chrome={runtime:{sendMessage:async()=>({ok:true,result:${JSON.stringify(draft)}}),onMessage:{addListener(){}}}};</script>`;
  const server = createServer(async (req, res) => {
    try {
      res.setHeader("Content-Type", req.url.endsWith(".js") ? "text/javascript" : req.url.endsWith(".css") ? "text/css" : "text/html");
      if (req.url === "/") {
        res.end(`<!doctype html><style>body{margin:0;background:#111;color:#ccc;display:flex;gap:16px}iframe{border:0;height:1200px;flex:none}pre{display:none}</style>${widths.map((width) => `<iframe title="${width}px panel" style="width:${width}px" src="/panel.html"></iframe>`).join("")}<pre id="proof"></pre><script>
        window.onload=()=>setTimeout(()=>{
          const proof=[...document.querySelectorAll('iframe')].map(frame=>{
            const d=frame.contentDocument;
            d.querySelectorAll('body > details').forEach(el=>el.open=true);
            const rect=el=>{const r=el.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom}};
            const groups=[d.querySelector('header .actions'),d.querySelector('#grant-capture').parentElement,d.querySelector('#server-list'),d.querySelector('footer'),d.querySelector('#errors .card'),d.querySelector('#cards .card')];
            return {width:frame.clientWidth,overflow:d.documentElement.scrollWidth,
              summaries:[...d.querySelectorAll('body > details > summary')].map(rect),
              groups:groups.map(el=>({name:el.id||el.className||el.tagName,box:rect(el),evidence:el.querySelector(':scope > details')?rect(el.querySelector(':scope > details')):null,buttons:[...el.querySelectorAll('button')].map(button=>({text:button.textContent,...rect(button)}))}))};
          });
          document.querySelector('#proof').textContent=JSON.stringify(proof);
        },500);
        </script>`);
      } else if (req.url === "/panel.html") res.end(html.replace('<script type="module"', `${stub}<script type="module"`));
      else if (["/panel.css", "/panel.js", "/browserCommands.js"].includes(req.url)) res.end(await readFile(new URL(req.url.slice(1), root)));
      else { res.statusCode = 404; res.end(); }
    } catch (error) { res.statusCode = 500; res.end(String(error)); }
  });
  try {
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const label = process.env.TUIC_PANEL_LABEL ?? "layout";
    const browser = spawn(process.env.TUIC_PANEL_CHROME, [
      "--headless=new", "--no-first-run", "--no-default-browser-check", "--disable-background-networking",
      "--use-mock-keychain", "--password-store=basic", "--remote-debugging-pipe",
      `--user-data-dir=${join(process.env.TUIC_TEST_TMP_ROOT, "chrome-profile")}`,
      "about:blank",
    ], { stdio: ["ignore", "ignore", "pipe", "pipe", "pipe"] });
    let sequence = 0;
    let buffer = "";
    let stderr = "";
    const pending = new Map();
    browser.stderr.on("data", (chunk) => { stderr += chunk; });
    browser.stdio[4].on("data", (chunk) => {
      buffer += chunk;
      let boundary;
      while ((boundary = buffer.indexOf("\0")) !== -1) {
        const message = JSON.parse(buffer.slice(0, boundary));
        buffer = buffer.slice(boundary + 1);
        const receiver = pending.get(message.id);
        if (receiver) { pending.delete(message.id); receiver(message); }
      }
    });
    const command = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
      const id = ++sequence;
      const deadline = setTimeout(() => { pending.delete(id); reject(new Error(`Chrome CDP timed out: ${method}`)); }, 30000);
      pending.set(id, (message) => { clearTimeout(deadline); if (message.error) reject(new Error(JSON.stringify(message.error))); else resolve(message.result); });
      browser.stdio[3].write(`${JSON.stringify({ id, method, params, sessionId })}\0`);
    });
    let proof;
    try {
      const { targetId } = await command("Target.createTarget", { url: "about:blank" });
      const { sessionId } = await command("Target.attachToTarget", { targetId, flatten: true });
      await command("Emulation.setDeviceMetricsOverride", { width: 1240, height: 1240, deviceScaleFactor: 1, mobile: false }, sessionId);
      await command("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/` }, sessionId);
      const result = await command("Runtime.evaluate", { expression: `new Promise((resolve,reject)=>{const deadline=Date.now()+15000;const timer=setInterval(()=>{const text=document.querySelector('#proof')?.textContent;if(text){clearInterval(timer);resolve(JSON.parse(text))}else if(Date.now()>deadline){clearInterval(timer);reject(new Error('Panel did not render'))}},50)})`, awaitPromise: true, returnByValue: true }, sessionId);
      assert.ok(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
      proof = result.result.value;
      const image = await command("Page.captureScreenshot", { format: "png", captureBeyondViewport: true }, sessionId);
      await writeFile(join(output, `${label}.png`), Buffer.from(image.data, "base64"));
    } finally {
      const stopped = new Promise((resolve) => browser.once("exit", resolve));
      browser.kill("SIGTERM");
      await stopped;
      await writeFile(join(output, `${label}-chrome.log`), stderr);
    }
    await writeFile(join(output, `${label}.json`), JSON.stringify(proof, null, 2));
    return proof;
  } finally { await new Promise((resolve) => server.close(resolve)); }
}

if (process.argv.includes("--capture-only")) {
  await capture();
} else {
  test("catches edge-aligned sections and touching permission/error buttons at narrow and wide panel widths", async () => {
    const proof = await capture();
    const failures = [];
    for (const panel of proof) {
      const check = (ok, message) => { if (!ok) failures.push(`${panel.width}px: ${message}`); };
      check(panel.overflow <= panel.width, "panel must not overflow horizontally");
      for (const summary of panel.summaries) check(summary.left === 16, `disclosure inset is ${summary.left}, expected 16`);
      for (const group of panel.groups) {
        check(group.buttons.length > 0, `${group.name} must render buttons`);
        const first = group.buttons[0];
        const nested = group.name === "card";
        if (group.evidence) check(first.top - group.evidence.bottom >= 8, "pick delete button touches evidence disclosure");
        check(first.left === (nested ? 29 : 16), `${group.name} button inset is ${first.left}`);
        for (let i = 1; i < group.buttons.length; i++) {
          const previous = group.buttons[i - 1];
          const current = group.buttons[i];
          const gap = current.top >= previous.bottom ? current.top - previous.bottom : current.left - previous.right;
          check(gap >= 8, `${group.name}: gap before '${current.text}' is ${gap}, expected >=8`);
          if (!nested && current.top >= previous.bottom) check(current.left === 16, `${group.name}: wrapped row loses content inset`);
        }
        for (const button of group.buttons) check(button.right <= panel.width - 16, `${group.name}: button crosses right inset`);
      }
    }
    assert.deepEqual(failures, [], failures.join("\n"));
  });
}
