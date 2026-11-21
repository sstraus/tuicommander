/** Real Chromium regression; run against the worktree Vite fixture. */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { browserWrapper } from "./browser-wrapper.mjs";
const wrapper = browserWrapper();
const base = process.env.MOBILE_LAYOUT_URL ?? "http://127.0.0.1:1447/scripts/fixtures/mobile-chat-layout.html";
function browser(...args) {
 const result = spawnSync("perl", ["-e", "alarm 45; exec @ARGV", wrapper, ...args], { encoding: "utf8", env: { ...process.env, AB_SESSION: process.env.AB_SESSION ?? "tuic-mobile-aichat-green" } });
 assert.equal(result.status, 0, result.stderr || result.stdout);
 return result.stdout.trim().split("\n").at(-1);
}
const measure = `(() => {
 document.querySelectorAll('details').forEach(d => { d.open = true; });
 const input = document.querySelector('textarea');
 const transcript = document.querySelector('[aria-label="Chat transcript"]');
 if (!input || !transcript) throw new Error('Layout fixture not mounted');
 const style = getComputedStyle(input);
 const properties = ['minHeight','maxHeight','padding','fontSize','fontFamily','lineHeight','borderRadius','backgroundColor','border','fieldSizing'];
 const send = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === 'Send' || b.getAttribute('aria-label') === 'Send');
 const attach = document.querySelector('[aria-label="Attach file"]');
 const emptyHeight = input.offsetHeight;
 input.value = 'Line one\\nLine two\\nLine three';
 input.dispatchEvent(new Event('input', {bubbles:true}));
 const grownHeight = input.offsetHeight;
 input.value = '';
 input.dispatchEvent(new Event('input', {bubbles:true}));
 const rect = e => { const r = e.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right}; };
 return {emptyHeight,grownHeight,width:transcript.clientWidth,scrollWidth:transcript.scrollWidth,input:rect(input),style:Object.fromEntries(properties.map(p=>[p,style[p]])),placeholder:{color:getComputedStyle(input,'::placeholder').color,opacity:getComputedStyle(input,'::placeholder').opacity},send:rect(send),round:getComputedStyle(send).borderRadius,background:getComputedStyle(send).backgroundColor,icon:send.querySelector('svg')?.outerHTML,attach:rect(attach),attachIcon:attach.querySelector('svg')?.outerHTML,actions:[...document.querySelector('section').querySelectorAll('button')].filter(b=>!transcript.contains(b)).map(b=>({text:b.textContent.trim(),label:b.getAttribute('aria-label'),icon:!!b.querySelector('svg')}))};
})()`;
const failures = [];
function check(name, action) { try { action(); console.log(`PASS ${name}`); } catch(error) { failures.push(`${name}: ${error.message}`); } }
for (const width of [390,360,430]) {
 browser(base);
 browser("--", "set", "viewport", String(width), "844");
 const ai = JSON.parse(JSON.parse(browser("--", "eval", `JSON.stringify(${measure})`)));
 check(`AI Chat transcript does not overflow at ${width}px`,()=>assert.ok(ai.scrollWidth <= ai.width, `scrollWidth ${ai.scrollWidth} > clientWidth ${ai.width}`));
 if (width !== 390) continue;
 browser(`${base}?view=session`);
 const session = JSON.parse(JSON.parse(browser("--", "eval", `JSON.stringify(${measure})`)));
 check("mobile composer shares session typography, sizing and placeholder",()=>{assert.deepEqual(ai.style,session.style);assert.deepEqual(ai.placeholder,session.placeholder);assert.equal(ai.input.height,session.input.height);assert.ok(ai.grownHeight>ai.emptyHeight);assert.equal(ai.grownHeight,session.grownHeight);});
 check("mobile composer keeps attachment right and identical round icon Send",()=>{assert.ok(ai.attach.x >= ai.input.right);assert.deepEqual(ai.send.width,session.send.width);assert.equal(ai.round,session.round);assert.equal(ai.background,session.background);assert.equal(ai.icon,session.icon);assert.equal(ai.attachIcon,session.attachIcon);});
 check("mobile composer extra actions use labelled icons",()=>{assert.ok(ai.actions.length>0);assert.ok(ai.actions.every(b=>!b.text&&b.label&&b.icon),JSON.stringify(ai.actions));});
}
if (failures.length) { console.error(failures.join("\n")); process.exitCode=1; }
