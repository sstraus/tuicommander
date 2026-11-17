import {spawn} from 'node:child_process';
import {mkdir,readFile,writeFile,unlink} from 'node:fs/promises';
import {testTmpRoot} from '../test-tmp-root.mjs';
// Browser state and PNGs live under the caller's temp dir, never inside the checkout.
const dir=testTmpRoot()+'/mobile-question-visual/'+(process.env.VISUAL_RUN??'capture')+'/';
await mkdir(dir,{recursive:true});
const failures=[];
await mkdir(dir+'chrome-profile',{recursive:true});await mkdir(dir+'png',{recursive:true});
await unlink(dir+'chrome-profile/DevToolsActivePort').catch(()=>{});
const chrome=spawn('/Users/stefano.straus/.agent-browser/browsers/chrome-149.0.7827.54/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',['--headless=new','--no-first-run','--disable-background-networking','--disable-component-update','--disable-sync','--remote-debugging-port=0','--user-data-dir='+dir+'chrome-profile','about:blank'],{stdio:['ignore','ignore','pipe']});
let ws;
try {
let port;for(let i=0;i<100;i++){try{port=(await readFile(dir+'chrome-profile/DevToolsActivePort','utf8')).split('\n')[0];break;}catch{await new Promise(r=>setTimeout(r,100));}}
const targets=await(await fetch(`http://127.0.0.1:${port}/json`)).json();ws=new WebSocket(targets.find(t=>t.type==='page').webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r,{once:true}));let seq=0;const pending=new Map();let errors=[];
ws.addEventListener('message',e=>{const x=JSON.parse(e.data);if(x.id){const p=pending.get(x.id);pending.delete(x.id);x.error?p.reject(new Error(JSON.stringify(x.error))):p.resolve(x.result);}else if(x.method==='Runtime.exceptionThrown')errors.push(x.params.exceptionDetails.exception?.description??x.params.exceptionDetails.text);else if(x.method==='Runtime.consoleAPICalled'&&x.params.type==='error')errors.push(x.params.args.map(a=>a.value??a.description).join(' '));else if(x.method==='Fetch.requestPaused'){const p=x.params;if(p.request.url.startsWith('http://127.0.0.1:15473/')||p.request.url.startsWith('data:'))send('Fetch.continueRequest',{requestId:p.requestId});else{send('Fetch.failRequest',{requestId:p.requestId,errorReason:'BlockedByClient'});}}});
function send(method,params={}){return new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});}
async function ev(expression){const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw new Error(r.exceptionDetails.exception?.description??r.exceptionDetails.text);return r.result.value;}
await send('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]});await send('Page.enable');await send('Runtime.enable');await send('Fetch.enable',{patterns:[{urlPattern:'*'}]});

if(process.env.VISUAL_VOICE){
 for(const width of [390,834])for(const theme of ['dark','light']){
  await send('Emulation.setDeviceMetricsOverride',{width,height:844,deviceScaleFactor:1,mobile:true});
  await send('Page.navigate',{url:`http://127.0.0.1:15473/scripts/mobile-question-visual/index.html?theme=${theme}&mode=single`});
  for(let i=0;i<100;i++){if(await ev("!!document.querySelector('[aria-label=\"More session actions\"]')"))break;await new Promise(r=>setTimeout(r,100));}
  if(width===390)await ev("document.querySelector('[aria-label=\"More session actions\"]').click()");
  const metrics=await ev(`(()=>{const b=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Start voice call'&&getComputedStyle(b).display!=='none')??document.querySelector('[aria-label="Start voice call"]');const r=b.getBoundingClientRect();return {handset:!!b.querySelector('svg'),x:r.x,y:r.y,width:r.width,height:r.height}})()`);
  const png=dir+`png/handset-${theme}-${width}.png`;
  await writeFile(png,Buffer.from((await send('Page.captureScreenshot',{format:'png'})).data,'base64'));
  console.log('SCREENSHOT ONLY',png,JSON.stringify(metrics));
 }
}else{
const results=[];
for(const width of [390,834])for(const theme of ['dark','light'])for(const mode of ['single','multiple','free_text']){
 await send('Emulation.setDeviceMetricsOverride',{width,height:844,deviceScaleFactor:1,mobile:true});
 await send('Page.navigate',{url:`http://127.0.0.1:15473/scripts/mobile-question-visual/index.html?theme=${theme}&mode=${mode}`});
 for(let i=0;i<100;i++){if(await ev("!!document.querySelector('[aria-label=\"Open ego question\"]')"))break;await new Promise(r=>setTimeout(r,100));}
 await ev("document.querySelector('[aria-label=\"Open ego question\"]').click()");
 await new Promise(r=>setTimeout(r,100));
 const metrics=await ev(`(()=>{const card=document.querySelector('[class*="_dropup_"], [class*="_question_"][role="status"]');const key=document.querySelector('button').closest('[class*="_screen_"]').querySelector('button[class*="_key_"]');const r=card.getBoundingClientRect(), k=key.getBoundingClientRect();const c=getComputedStyle(card);const text=getComputedStyle(card.querySelector('button')??card);return {bottom:r.bottom,keyTop:k.top,background:c.backgroundColor,foreground:text.color,scrollWidth:document.documentElement.scrollWidth}})()`);
 const rgb=s=>s.match(/[\d.]+/g).slice(0,3).map(Number);
 const lum=s=>rgb(s).map(v=>v/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4).reduce((a,v,i)=>a+v*[.2126,.7152,.0722][i],0);
 const a=lum(metrics.background),b=lum(metrics.foreground);metrics.contrast=(Math.max(a,b)+.05)/(Math.min(a,b)+.05);
 if(!process.env.VISUAL_ONLY && (metrics.contrast<4.5||metrics.bottom>metrics.keyTop||metrics.scrollWidth>width))failures.push({width,theme,mode,...metrics});
 if(errors.length)throw new Error('Question fixture errors: '+JSON.stringify(errors));
 const png=dir+`png/${mode}-${theme}-${width}.png`;
 await writeFile(png,Buffer.from((await send('Page.captureScreenshot',{format:'png'})).data,'base64'));
 results.push({width,theme,mode,metrics,png});
}
await writeFile(dir+'assertions.json',JSON.stringify({test:'mobile_question_cards_keep_theme_contrast_and_clear_keybar',results,failures},null,2));
console.log('mobile_question_cards_keep_theme_contrast_and_clear_keybar',process.env.VISUAL_ONLY?'SCREENSHOTS ONLY':failures.length?'FAIL':'PASS',JSON.stringify(failures));if(failures.length)process.exitCode=1;
}
}finally{ws?.close();chrome.kill('SIGTERM');}
