import {spawn,spawnSync} from 'node:child_process';
import {createWriteStream} from 'node:fs';
import {lockProbeFailure} from './lock-probe.mjs';
import {mkdir,readFile,writeFile,unlink} from 'node:fs/promises';
import {testTmpRoot} from '../test-tmp-root.mjs';
const source=new URL('./',import.meta.url).pathname;
// Screenshots and reports live under the caller's temp dir, never inside the checkout.
const dir=testTmpRoot()+'/settings-visual/'+(process.env.VISUAL_RUN??'capture')+'/';
await mkdir(dir,{recursive:true});
if(process.platform==='darwin'){
 const lock=spawnSync('ioreg',['-n','Root','-d1','-a'],{encoding:'utf8',timeout:10000});
 const reason=lockProbeFailure(lock);
 if(reason){
  // DEFERRED (2026-10-18) — macOS pauses Chrome rendering while locked; screenshots need an unlocked display.
  const error='Visual prerequisite unavailable: '+reason+'. No UI assertion ran.';
  await writeFile(dir+'prerequisite-failure.json',JSON.stringify({error},null,2));
  throw new Error(error);
 }
}
const test=process.env.VISUAL_TEST; const failures=[];
await mkdir(dir+'chrome-profile',{recursive:true});await mkdir(dir+'png',{recursive:true});
await unlink(dir+'chrome-profile/DevToolsActivePort').catch(()=>{});
const chrome=spawn('/Users/stefano.straus/.agent-browser/browsers/chrome-149.0.7827.54/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',['--headless=new','--no-first-run','--disable-background-networking','--disable-component-update','--disable-sync','--remote-debugging-port=0','--user-data-dir='+dir+'chrome-profile','about:blank'],{stdio:['ignore','ignore','pipe']});
chrome.stderr.pipe(createWriteStream(dir+'chrome-stderr.log'));
let ws;const pending=new Map();
try {
let port;for(let i=0;i<100;i++){try{port=(await readFile(dir+'chrome-profile/DevToolsActivePort','utf8')).split('\n')[0];break;}catch{await new Promise(r=>setTimeout(r,100));}}
const targets=await(await fetch(`http://127.0.0.1:${port}/json`)).json();ws=new WebSocket(targets.find(t=>t.type==='page').webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r,{once:true}));let seq=0;let errors=[],network=[];
ws.addEventListener('message',e=>{const x=JSON.parse(e.data);if(x.id){const p=pending.get(x.id);if(!p)return;pending.delete(x.id);clearTimeout(p.timer);x.error?p.reject(new Error(JSON.stringify(x.error))):p.resolve(x.result);}else if(x.method==='Runtime.exceptionThrown')errors.push(x.params.exceptionDetails.exception?.description??x.params.exceptionDetails.text);else if(x.method==='Runtime.consoleAPICalled'&&x.params.type==='error')errors.push(x.params.args.map(a=>a.value??a.description).join(' '));else if(x.method==='Fetch.requestPaused'){const p=x.params;if(p.request.url.startsWith('http://127.0.0.1:15391/')||p.request.url.startsWith('data:'))send('Fetch.continueRequest',{requestId:p.requestId});else{network.push(p.request.url);send('Fetch.failRequest',{requestId:p.requestId,errorReason:'BlockedByClient'});}}});
// Outer harness hang bound, not an assertion about Settings response latency.
function send(method,params={}){return new Promise((resolve,reject)=>{const id=++seq;const timer=setTimeout(()=>{pending.delete(id);reject(new Error('Visual harness CDP response timed out: '+method));},60000);pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method,params}));});}
async function ev(expression){const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw new Error(r.exceptionDetails.exception?.description??r.exceptionDetails.text);return r.result.value;}
await send('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]});await send('Page.enable');await send('Runtime.enable');await send('Fetch.enable',{patterns:[{urlPattern:'*'}]});
const tabs=process.env.VISUAL_PAGES?process.env.VISUAL_PAGES.split(','):test==='settings_text_contrast_on_themed_surfaces'?['general','terminal','keyboard-shortcuts','ai-chat','plugins','repo:/workspace/fixture-project']:test==='settings_agent_initials_contrast'?['agents']:test==='settings_appearance_legend_light_text_contrast'?['appearance']:test==='settings_help_badges_share_label_line'?['general','notifications','github','agents','repo:/workspace/fixture-project']:['general','appearance','notifications','terminal','keyboard-shortcuts','github','agents','ai-chat','dictation','smart-prompts','mcp','remote-access','remote-servers','telegram','plugins','repo:/workspace/fixture-project'];
// Custom launchers are desktop-only. Keep the scenario and its assertion for
// desktop captures; browser mode must not expect controls the app deliberately hides.
const launcherScenarios=process.env.VISUAL_BROWSER==='1'?[]:[{tab:'general',state:'launcher-editor'}];
const results=[];
for(const width of [1400,390])for(const theme of ['dark','light'])for(const entry of [...tabs.map(tab=>({tab,state:'ready'})),{tab:'plugin:fixture',state:'ready'},{tab:'plugin:fixture',state:'loading'},{tab:'plugin:fixture',state:'error'},{tab:'plugin:fixture',state:'stale'},{tab:'ai-chat',state:'nested'},...launcherScenarios,...['general','ai-chat','repo:/workspace/fixture-project'].map(tab=>({tab,state:'long-text'}))]){const {tab,state}=entry;errors=[];network=[];await send('Emulation.setDeviceMetricsOverride',{width,height:width===1400?900:844,deviceScaleFactor:1,mobile:false});const url=`http://127.0.0.1:15391/scripts/settings-visual/index.html?theme=${theme}&tab=${encodeURIComponent(tab)}&state=${state}${process.env.VISUAL_BROWSER==='1'?'&browser=1':''}`;
await send('Page.navigate',{url});
let settled=false;
for(let ready=0;ready<100;ready++){
 try{settled=await ev(`location.href===${JSON.stringify(url)}&&document.readyState==='complete'&&!!document.querySelector('[data-settings-content] header')&&document.documentElement.dataset.theme===${JSON.stringify(theme==='light'?'vscode-light':'commander')}&&getComputedStyle(document.body).color===${JSON.stringify(theme==='light'?'rgb(51, 51, 51)':'rgb(212, 212, 212)')}`);}catch{settled=false;}
 if(settled)break;await new Promise(r=>setTimeout(r,100));
}
if(!settled){
 const diagnostic={url,errors,blockedNetwork:network,page:await ev(`({url:location.href,ready:document.readyState,text:document.body.innerText,theme:document.documentElement.dataset.theme,foreground:getComputedStyle(document.body).color})`)};
 await writeFile(dir+'setup-failure.json',JSON.stringify(diagnostic,null,2));
 await writeFile(dir+'setup-failure.png',Buffer.from((await send('Page.captureScreenshot',{format:'png'})).data,'base64'));
 throw new Error('Requested Settings document/theme did not settle: '+JSON.stringify(diagnostic));
}
await ev(`document.fonts.ready.then(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))))`);
// Inherited text may still be black on the first mounted frame while Vite installs styles.
// Read the actual targets, not just root tokens, before accepting the document.
let textSettled=false;
for(let ready=0;ready<100;ready++){
 textSettled=await ev(`(()=>{const c=document.querySelector('[data-settings-content]');const foreground=getComputedStyle(document.body).color;const h=c.querySelector('header h2');const badges=[...c.querySelectorAll('[data-testid=status-badge]')];return getComputedStyle(h).color===foreground&&badges.every(e=>getComputedStyle(e).color!=='rgb(0, 0, 0)');})()`);
 if(textSettled)break;await new Promise(r=>setTimeout(r,100));
}
if(!textSettled)throw new Error('Settings text theme did not settle: '+url);
await ev(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);


if(tab==='all-settings'){
 await ev(`(()=>{const select=document.querySelector('[data-settings-content] select');select.value='app.shell';select.dispatchEvent(new Event('change',{bubbles:true}));})()`);
 let mounted=false;for(let ready=0;ready<100;ready++){mounted=await ev(`!!document.querySelector('input[data-setting-id="app.shell"]')&&document.body.innerText.includes('Source: visual fixture')`);if(mounted)break;await new Promise(r=>setTimeout(r,50));}
 if(!mounted)throw new Error('Expert inspector did not render its descriptor-bound owner read');
}
if(state==='long-text')await ev(`(()=>{const c=document.querySelector('[data-settings-content]');const label=c.querySelector('label');label?.append(' — Long translated configuration label that must wrap without overlapping its help badge or control');const input=[...c.querySelectorAll('input')].find(e=>e.type==='text');if(input)input.value='/workspace/long-repository-name/long-profile-name/long-configuration-value';})()`);
if(state==='nested')await ev(`(()=>{[...document.querySelectorAll('button')].find(e=>e.textContent==='Add folder')?.click();})()`);
if(state==='launcher-editor')await ev(`(()=>{[...document.querySelectorAll('button')].find(e=>e.textContent==='Add launcher')?.click();})()`);
if(state==='stale'){
 await ev(`(()=>{const input=document.querySelector('[data-settings-content] input[type=text]');input.focus();input.select();})()`);
 await send('Input.insertText',{text:'https://changed.example.test'});
 await send('Input.dispatchKeyEvent',{type:'keyDown',key:'Tab',code:'Tab',windowsVirtualKeyCode:9});
 await send('Input.dispatchKeyEvent',{type:'keyUp',key:'Tab',code:'Tab',windowsVirtualKeyCode:9});
 await new Promise(r=>setTimeout(r,150));
}
let metrics;try{metrics=await ev(`(()=>{const content=document.querySelector('[data-settings-content]');return {title:document.title,text:document.body.innerText,content:content?{clientWidth:content.clientWidth,scrollWidth:content.scrollWidth,clientHeight:content.clientHeight,scrollHeight:content.scrollHeight}:null,calls:window.auditCalls,theme:{name:document.documentElement.dataset.theme,foreground:getComputedStyle(document.body).color,heading:getComputedStyle(content.querySelector('h2')).color,updateBadge:[...content.querySelectorAll('[data-testid=status-badge]')].filter(e=>e.textContent==='Update status').map(e=>getComputedStyle(e).color)}}})()`);}catch(e){metrics={evaluationError:e.message};}
const name=tab.split(':')[0]+'-'+theme+'-'+width+'-'+state;const png=[];
if(metrics.content){const count=Math.ceil(metrics.content.scrollHeight/Math.max(metrics.content.clientHeight-40,1));for(let i=0;i<count;i++){await ev(`document.querySelector('[data-settings-content]').scrollTop=${i*(metrics.content.clientHeight-40)}`);await new Promise(r=>setTimeout(r,40));const p=dir+'png/'+name+'-'+i+'.png';const shot=await send('Page.captureScreenshot',{format:'png'});await writeFile(p,Buffer.from(shot.data,'base64'));png.push(p);}}
else{const p=dir+'png/'+name+'-error.png';await writeFile(p,Buffer.from((await send('Page.captureScreenshot',{format:'png'})).data,'base64'));png.push(p);}
const preferred={general:'Explain ego executable',notifications:'Explain Orchestration',github:'Explain Default Setup Script',agents:'Explain Filesystem sandbox','repo:/workspace/fixture-project':'Explain Setup Script'}[tab];
const help=await ev(`(()=>{const buttons=[...document.querySelectorAll('[data-settings-content] [role=button][aria-describedby]')];const b=buttons.find(e=>e.getAttribute('aria-label')===${JSON.stringify(preferred??'')})??buttons[0];if(!b)return null;b.scrollIntoView({block:'center'});b.click();const t=document.getElementById(b.getAttribute('aria-describedby'));const r=t.getBoundingClientRect();return {label:b.getAttribute('aria-label'),visible:t.dataset.visible==='true',inViewport:r.x>=0&&r.right<=innerWidth&&r.y>=0&&r.bottom<=innerHeight}})()`);
if(help){const p=dir+'png/'+name+'-help.png';await writeFile(p,Buffer.from((await send('Page.captureScreenshot',{format:'png'})).data,'base64'));png.push(p);}
const geometry=metrics.content?await ev(await readFile(source+'geometry.js','utf8')):null;
if(geometry){const wanted={loading:'Loading plugin settings',error:'Could not read plugin settings',stale:'Setting was not saved',nested:'Hide folder editor','launcher-editor':'1 custom launchers'}[state];if(wanted&&!geometry.text.includes(wanted))failures.push({tab,state,theme,width,name:'scenario rendered',wanted,text:geometry.text});for(const c of geometry.checks)if(!c.pass)failures.push({tab,state,theme,width,...c});const previous=results.find(r=>r.width===width&&r.theme===theme&&r.tab==='general'&&r.state==='ready');if(previous&&Math.abs(previous.geometry.header.width-geometry.header.width)>1)failures.push({tab,state,theme,width,name:'equal page header widths',expected:previous.geometry.header.width,actual:geometry.header.width});}
const details=metrics.content?await ev(await readFile(source+'inspect.js','utf8')):null;
if(process.env.VISUAL_ELEMENTS){const selectors='[class*="_helpLabel_"], [class*="_agentIcon_"], [class*="_prBadgeCompact_"]';const count=await ev(`document.querySelectorAll(${JSON.stringify(selectors)}).length`);for(let i=0;i<count;i++){const rect=await ev(`(()=>{const e=document.querySelectorAll(${JSON.stringify(selectors)})[${i}];e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect();return {x:Math.max(0,r.x-4),y:Math.max(0,r.y-4),width:Math.min(innerWidth-r.x-4,r.width+8),height:Math.min(innerHeight-r.y-4,r.height+8),scale:1}})()`);if(rect.width>0&&rect.height>0){const p=dir+'png/'+name+'-element-'+i+'.png';await writeFile(p,Buffer.from((await send('Page.captureScreenshot',{format:'png',clip:rect})).data,'base64'));png.push(p);}}}
if(!metrics.content||errors.length)throw new Error('Settings did not mount cleanly: '+JSON.stringify({tab,errors,metrics}));
if(test && details){let bad=[]; if(test==='settings_text_contrast_on_themed_surfaces')bad=details.contrast.filter(x=>!['?','#42','G','P','C','X','D','A','O','E'].includes(x.text)); if(test==='settings_agent_initials_contrast')bad=details.contrast.filter(x=>['G','P'].includes(x.text)); if(test==='settings_appearance_legend_light_text_contrast')bad=details.contrast.filter(x=>['?','#','42','#42'].includes(x.text)); if(test==='settings_help_badges_share_label_line')bad=details.badges.filter(x=>x.ownLine); if(bad.length)failures.push({tab,theme,width,bad});}
results.push({tab,state,theme,width,geometry,errors:[...errors],blockedNetwork:[...network],metrics,details,help,png});await writeFile(dir+'results-'+width+'-'+theme+'.json',JSON.stringify(results.filter(r=>r.width===width&&r.theme===theme),null,2));console.log(name,errors.length,metrics.content?JSON.stringify(metrics.content):'NO CONTENT');if(errors.length&&!process.argv.includes('--all'))console.log(errors);
}
await writeFile(dir+'geometry-assertions.json',JSON.stringify({failures},null,2));
if(failures.length)process.exitCode=1;
if(test){await writeFile(dir+'assertions.json',JSON.stringify({test,failures},null,2)); console.log(test, failures.length?'FAIL':'PASS',JSON.stringify(failures)); if(failures.length)process.exitCode=1;}
}finally{for(const request of pending?.values()??[])clearTimeout(request.timer);ws?.close();chrome.kill('SIGTERM');}
