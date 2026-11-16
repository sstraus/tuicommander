// Catches: viewport-sized overflow menus whose final actions are clipped by their session pane.
// Uses the real session screen/CSS in an isolated Vite fixture, never a live backend.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { createServer } from "vite";

const run = promisify(execFile);
const evidence = path.resolve(process.argv[2]);
// Evidence stays under the test temp root, never an arbitrary caller-supplied directory.
assert(evidence.startsWith(path.resolve(process.env.TUIC_TEST_TMP_ROOT || tmpdir()) + path.sep));
await mkdir(evidence, { recursive: true });
await mkdir(path.join(evidence, "reap-scope"), { recursive: true });
// The stealth browser wrapper (the global rules' agent-browser entry point) is machine-specific.
const wrapper = process.env.TUIC_BROWSER_WRAPPER;
assert(wrapper, "set TUIC_BROWSER_WRAPPER to the stealth agent-browser wrapper");
const session = `session-menu-${Date.now()}`;
const env = {...process.env, AB_SESSION:session, AB_AGENT_BROWSER_DIR:path.join(evidence,"reap-scope")};
const server = await createServer({configFile:"src/mobile/__tests__/browser/sessionUsage.vite.config.ts",server:{port:0}});
let opened = false;
const transcript = [];
async function browser(...args) {
 const result = await run("perl", ["-e","alarm 35; exec @ARGV",wrapper,...args],{env,timeout:37000,maxBuffer:1024*1024});
 transcript.push({args,stdout:result.stdout,stderr:result.stderr});
 return result.stdout.trim();
}
async function evaluate(code) { return JSON.parse(JSON.parse(await browser("--","eval",`JSON.stringify(${code})`))); }
try {
 await server.listen();
 const address=server.httpServer.address();
 await browser(`http://127.0.0.1:${address.port}/src/mobile/__tests__/browser/sessionUsage.html?state=quota`);
 opened=true;
 await browser("--","set","viewport","390","844");
 await browser("--","eval",`(()=>{const button=document.querySelector('[aria-label="More session actions"]');if(button?.getAttribute('aria-expanded')!=='true')button?.click();return document.body.innerText})()`);
 await browser("--","eval",`new Promise((resolve,reject)=>{const ready=()=>document.querySelector('[aria-label="Session actions"]')?.textContent.includes('Claude Max plan');if(ready())return resolve(true);const o=new MutationObserver(()=>{if(ready()){o.disconnect();resolve(true)}});o.observe(document.body,{childList:true,subtree:true});setTimeout(()=>{o.disconnect();reject(new Error('menu did not load'))},15000)})`);
 await browser("--","screenshot",path.join(evidence,"menu-top.png"));
 const geometry=await evaluate(`(()=>{const menu=document.querySelector('[aria-label="Session actions"]');menu.scrollTop=menu.scrollHeight;const rect=e=>{const r=e.getBoundingClientRect();return {top:r.top,bottom:r.bottom,height:r.height}};const copy=[...menu.querySelectorAll('button')].find(b=>b.textContent==='Copy session ID');const last=[...menu.querySelectorAll('button')].at(-1);return {viewport:[innerWidth,innerHeight],pane:rect(menu.parentElement),menu:rect(menu),copy:rect(copy),last:rect(last),clientHeight:menu.clientHeight,scrollHeight:menu.scrollHeight,scrollTop:menu.scrollTop}})()`);
 await browser("--","screenshot",path.join(evidence,"menu-bottom.png"));
 await writeFile(path.join(evidence,"geometry.json"),JSON.stringify(geometry,null,2));
 assert.deepEqual(geometry.viewport,[390,844]);
 assert(geometry.scrollHeight > geometry.clientHeight,"fixture must exercise scrolling");
 assert(geometry.menu.bottom <= geometry.pane.bottom,"menu extends below available session pane");
 for (const action of [geometry.copy,geometry.last]) {
  assert(action.top >= geometry.menu.top && action.bottom <= geometry.menu.bottom,"final action is not fully reachable");
 }
 process.stdout.write(JSON.stringify(geometry)+"\n");
} catch (error) {
 try { await browser("--","snapshot"); await browser("--","screenshot",path.join(evidence,"failure.png")); } catch {}
 throw error;
} finally {
 if(opened) await browser("--","tab","close").catch(error=>transcript.push({cleanup:error.message}));
 const chrome = transcript[0]?.stderr?.match(/Chrome=(\d+)/)?.[1];
 if (chrome) await run("agent-browser",["--session",`ab-shared-${session}-${chrome}`,"close"],{env,timeout:15000}).catch(error=>transcript.push({cleanup:error.message}));
 await server.close();
 await writeFile(path.join(evidence,"transcript.json"),JSON.stringify(transcript,null,2));
}
