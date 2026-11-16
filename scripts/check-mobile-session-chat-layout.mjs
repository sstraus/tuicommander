import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { browserWrapper } from "./browser-wrapper.mjs";
const wrapper = browserWrapper();
const evidence = process.env.MOBILE_CHAT_EVIDENCE;
assert.ok(evidence, "MOBILE_CHAT_EVIDENCE is required");
mkdirSync(evidence, { recursive: true });
const base = process.env.MOBILE_SESSION_LAYOUT_URL;
assert.ok(base, "MOBILE_SESSION_LAYOUT_URL is required");
function browser(...args) {
 const result = spawnSync("perl", ["-e", "alarm 45; exec @ARGV", wrapper, ...args], { encoding: "utf8", env: { ...process.env, AB_SESSION: "feat1770d" } });
 assert.equal(result.status, 0, result.stderr || result.stdout);
 return result.stdout.trim().split("\n").at(-1);
}
function evaluate(js) { return JSON.parse(JSON.parse(browser("--", "eval", `JSON.stringify(${js})`))); }
const results = [];
try {
 for (const width of [360,390,430]) {
  browser(base); browser("--", "snapshot"); browser("--", "set", "viewport", String(width), "800");
  const baseline = evaluate(`(() => {const h=document.querySelector('header').getBoundingClientRect();return {height:h.height,bodyWidth:document.documentElement.scrollWidth};})()`);
  // Only snapshot rendering is synthetic. Sessions, output WS and existing /events SSE use the isolated daemon.
  evaluate(`(() => {
   const original=window.fetch.bind(window); window.benchmarkReads=0;
   window.fetch=async (input, init) => {
    if(String(input).includes('/chat-view')) {
     window.benchmarkReads++;
     if(window.benchmarkReads > 1 && !window.fullReset) return new Response(JSON.stringify({epoch:1,nextSeq:2000,reset:false,updates:[],unknownRows:0,malformedRows:0}),{headers:{"content-type":"application/json"}});
     window.fullReset=false;
     const updates=Array.from({length:2000},(_,i)=>({sessionUpdate:i%2?'agent_message_chunk':'user_message_chunk',messageId:'entry-'+i,content:{type:'text',text:(i%2?'Answer ':'Question ')+i+' — bounded transcript rendering benchmark.'}}));
     return new Response(JSON.stringify({epoch:1,nextSeq:2000,reset:true,updates,unknownRows:0,malformedRows:0}),{status:200,headers:{'content-type':'application/json'}});
    }
    return original(input,init);
   };
   return true;
  })()`);
  evaluate(`(() => {
   window.benchmarkStart=performance.now();
   const observer=new MutationObserver(()=>{
    if(document.querySelector('[aria-label="Chat transcript"]')?.textContent.includes('Answer 1999')) {
     observer.disconnect();window.benchmarkMs=performance.now()-window.benchmarkStart;
    }
   });observer.observe(document.body,{childList:true,subtree:true,characterData:true});
   [...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Chat').click();return true;
  })()`);
  browser("--", "snapshot");
  const first = evaluate(`(() => {
   const transcript=document.querySelector('[aria-label="Chat transcript"]');
   if(!transcript) return {missing:true,text:document.body.textContent,reads:window.benchmarkReads,fetch:String(window.fetch).slice(0,60)};
   const header=document.querySelector('header');const toggle=document.querySelector('[aria-label="Terminal view"]');
   const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,right:r.right,width:r.width,height:r.height};};
   return {ms:window.benchmarkMs,entries:transcript.querySelectorAll('[class*="userMsg"], [class*="assistantMsg"]').length,header:rect(header),toggle:rect(toggle),width:transcript.clientWidth,scrollWidth:transcript.scrollWidth,bodyWidth:document.documentElement.scrollWidth,reads:window.benchmarkReads};
  })()`);
  console.log(JSON.stringify(first));
  assert.equal(first.header.height, baseline.height);
  assert.ok(first.toggle.right <= width);
  assert.ok(first.bodyWidth <= width);
  assert.ok(first.scrollWidth <= first.width);
  browser("--", "screenshot", `${evidence}/session-chat-${width}.png`);
  evaluate(`(() => {
   let hidden=true;
   Object.defineProperty(document,'visibilityState',{configurable:true,get:()=>hidden?'hidden':'visible'});
   document.dispatchEvent(new Event('visibilitychange'));
   // Snapshot response always resets, reproducing the worst-case expired viewer full reset without sleeping 20s.
   window.fullReset=true;window.benchmarkMs=null;window.benchmarkStart=performance.now();
   const observer=new MutationObserver(()=>{observer.disconnect();window.benchmarkMs=performance.now()-window.benchmarkStart;});
   observer.observe(document.body,{childList:true,subtree:true,characterData:true});
   hidden=false;document.dispatchEvent(new Event('visibilitychange'));return true;
  })()`);
  const resume=evaluate(`({ms:window.benchmarkMs,reads:window.benchmarkReads})`);
  assert.ok(resume.reads > first.reads);
  results.push({width,baseline,first,resume});
 }
 writeFileSync(`${evidence}/measurements.json`, JSON.stringify({scope:'Chromium mobile viewport, synthetic 2000-entry reset snapshots; isolated rb headless real session/output/SSE, not iPhone or backend tail measurement',results},null,2));
 console.log(JSON.stringify(results));
} finally { browser("--", "tab", "close"); }
