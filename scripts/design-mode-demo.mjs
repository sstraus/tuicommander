// Opt-in evidence driver for design_mode::demo. Uses only the agent-owned CFT profile.
import {readFile,writeFile,readdir,copyFile,mkdir} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import {join} from 'node:path';
import {browserWrapper} from './browser-wrapper.mjs';
const [socket,repo,output]=process.argv.slice(2);
const profile=process.env.TUIC_DESIGN_DEMO_PROFILE;
const bridge=process.env.TUIC_DESIGN_BRIDGE;
if(!profile||!bridge) throw Error('Set TUIC_DESIGN_DEMO_PROFILE and TUIC_DESIGN_BRIDGE to owned test paths.');
const id='eefbgjmmpaimpeaflioojmdipmfchefo';
// The unpacked copy lives beside the agent-owned profile, never inside the checkout.
const product=join(profile,'product-extension');
const hostConfig=join(profile,'tuic-design-host','tuic-design-host.json');
const binding=join(product,'binding.json');
const savedConfig=await readFile(hostConfig);const savedBinding=await readFile(binding);
const port=(await readFile(join(profile,'DevToolsActivePort'),'utf8')).split('\n')[0];
const list=async()=> (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const pause=()=>new Promise(resolve=>setTimeout(resolve,80));
async function connect(target){
 const ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
 let id=0;const pending=new Map();
 ws.onmessage=event=>{const message=JSON.parse(event.data);const request=pending.get(message.id);if(request){pending.delete(message.id);clearTimeout(request.timer);message.error?request.reject(message.error):request.resolve(message.result);}};
 return {send(method,params={}){return new Promise((resolve,reject)=>{const key=++id;const timer=setTimeout(()=>{pending.delete(key);reject(Error(`CDP timeout ${method}`));},15000);pending.set(key,{resolve,reject,timer});ws.send(JSON.stringify({id:key,method,params}));});},close(){for(const request of pending.values()){clearTimeout(request.timer);request.reject(Error('CDP closed'));}pending.clear();ws.close();}};
}
const evaluate=async(client,expression)=>(await client.send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true})).result?.value;
async function click(client,selector){const r=await evaluate(client,`document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect().toJSON()`);for(const type of ['mousePressed','mouseReleased'])await client.send('Input.dispatchMouseEvent',{type,x:r.x+Math.min(r.width/2,10),y:r.y+Math.min(r.height/2,10),button:'left',clickCount:1});}
async function until(client,expression){const deadline=Date.now()+20000;while(Date.now()<deadline){if(await evaluate(client,expression))return;await pause();}throw Error(`Browser state missing: ${expression}`);}
async function reloadWorker(){const target=(await list()).find(t=>t.type==='service_worker'&&t.url.includes(id));if(target){const worker=await connect(target);await evaluate(worker,'setTimeout(()=>chrome.runtime.reload(),10)');worker.close();await pause();}}
let panel,page;
try {
 const installed=spawnSync(bridge,['--design-install',profile,repo,socket,process.env.TUIC_DESIGN_DEMO_SESSION],{encoding:'utf8'});
 if(installed.status!==0)throw Error(installed.stderr);
 await mkdir(product,{recursive:true});for(const file of await readdir(join(profile,'tuic-design-extension')))await copyFile(join(profile,'tuic-design-extension',file),join(product,file));
 await reloadWorker();
 const wrapper=browserWrapper();
 const opened=spawnSync(wrapper,[`chrome-extension://${id}/panel.html`],{env:{...process.env,AB_SESSION:'tuic-1672-demo',AB_SHARED_PROFILE:profile},encoding:'utf8',timeout:45000});
 if(opened.status!==0)throw Error(opened.stderr);
 const extension=await connect((await list()).find(t=>t.url===`chrome-extension://${id}/panel.html`));
 await evaluate(extension,`chrome.storage.local.set({drafts:{}})`);
 await evaluate(extension,`chrome.windows.getCurrent().then(w=>{const button=document.createElement('button');button.id='demo-open';button.textContent='Open sidebar';button.onclick=()=>chrome.sidePanel.open({windowId:w.id});document.body.prepend(button);})`);
 await click(extension,'#demo-open');extension.close();await pause();
 for(const target of (await list()).filter(t=>t.url===`chrome-extension://${id}/panel.html`)){const client=await connect(target);if(await evaluate(client,'innerWidth<500')){panel=client;break;}client.close();}
 if(!panel)throw Error('Browser sidebar did not open');
 page=await connect((await list()).find(t=>t.url==='http://127.0.0.1:19340/'));
 await page.send('Page.bringToFront');await page.send('Page.reload');await until(page,'document.readyState==="complete"');
 await until(panel,`document.querySelector('#agent').value===${JSON.stringify(process.env.TUIC_DESIGN_DEMO_SESSION)}`);
 for(const [index,selector,comment] of [[0,'#target','Give this button more breathing room'],[1,'h1','Make the heading easier to scan']]) {
  await click(panel,'#pick');await pause();await click(page,selector);await until(panel,`document.querySelectorAll('textarea').length===${index+1}`);
  await evaluate(panel,`(()=>{const field=document.querySelectorAll('textarea')[${index}];field.value=${JSON.stringify(comment)};field.dispatchEvent(new Event('input',{bubbles:true}));field.blur();})()`);await pause();
 }
 await until(panel,'!document.querySelector("#send").disabled');
 await mkdir(output,{recursive:true});
 await evaluate(panel,"document.querySelector('#cards').scrollIntoView({block:'start'})");
 await writeFile(join(output,'two-commented-picks.png'),Buffer.from((await panel.send('Page.captureScreenshot')).data,'base64'));
 await writeFile(join(output,'sidebar-before-send.txt'),await evaluate(panel,'document.body.innerText'));
 await click(panel,'#send');await until(panel,'document.querySelector("#status").textContent.includes("Delivered now")');
 await writeFile(join(output,'delivery.png'),Buffer.from((await panel.send('Page.captureScreenshot')).data,'base64'));
 await writeFile(join(output,'sidebar-after-send.txt'),await evaluate(panel,'document.body.innerText'));
 console.log('Demo sent two commented picks through the native host and backend idle gate.');
} finally {
 panel?.close();page?.close();await writeFile(hostConfig,savedConfig);await writeFile(binding,savedBinding);await reloadWorker();
}
