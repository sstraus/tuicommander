import {createServer} from 'vite';
import solid from 'vite-plugin-solid';
import {spawn} from 'node:child_process';
import {writeFile,mkdir,readFile,readdir} from 'node:fs/promises';
const out=process.env.TUIC_ICON_EVIDENCE ?? '.tmp/visual-icon-geometry';
await mkdir(out,{recursive:true});
const phase=process.env.TUIC_ICON_PHASE ?? 'after';
const s=await createServer({configFile:false,plugins:[solid(),{name:'themes',configureServer(server){server.middlewares.use('/__tuic_themes',async(_req,res)=>{const dir='src-tauri/src/themes';const files=await readdir(dir);res.setHeader('content-type','application/json');res.end(JSON.stringify(await Promise.all(files.filter(f=>f.endsWith('.json')).map(async f=>({key:f.slice(0,-5),...JSON.parse(await readFile(`${dir}/${f}`,'utf8'))})))));});}}],define:{__APP_VERSION__:'"2.0.0"',__BUILD_GIT_HASH__:'"audit"'},cacheDir:'.tmp/visual-audit/vite',server:{host:'127.0.0.1',port:0},logLevel:'error'});
let chrome,ws;
try{
await s.listen();
chrome=spawn(process.env.TUIC_TEST_CHROME,['--headless=new','--disable-gpu','--no-first-run','--remote-debugging-port=0',`--user-data-dir=${out}/chrome-${phase}`,'about:blank'],{stdio:['ignore','ignore','pipe']});
const endpoint=await new Promise((ok,fail)=>{let text='';chrome.stderr.on('data',v=>{text+=v;const m=text.match(/DevTools listening on (ws:\/\/\S+)/);if(m)ok(m[1]);});chrome.once('error',fail);chrome.once('exit',c=>fail(Error(`chrome exit ${c}`)));});
ws=new WebSocket(endpoint);await new Promise(ok=>ws.addEventListener('open',ok,{once:true}));let id=0;const pending=new Map();ws.addEventListener('message',e=>{const m=JSON.parse(e.data);const p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}});const send=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params,sessionId}));});
const{targetId}=await send('Target.createTarget',{url:'about:blank'});const{sessionId}=await send('Target.attachToTarget',{targetId,flatten:true});const call=(m,p)=>send(m,p,sessionId);const evaluate=async expression=>{const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
await call('Emulation.setDeviceMetricsOverride',{width:1400,height:1000,deviceScaleFactor:1,mobile:false});
const samples=[];
for(const [name,path,ready] of [['icons','src/__tests__/browser/agentIconAppearance.html?theme=dark','window.auditReady'],['icons-light','src/__tests__/browser/agentIconAppearance.html?theme=light','window.auditReady']]){
await call('Page.navigate',{url:`http://127.0.0.1:${s.httpServer.address().port}/${path}`});
await evaluate(`new Promise((ok,fail)=>{const deadline=Date.now()+25000;const tick=()=>{if(${ready})requestAnimationFrame(()=>requestAnimationFrame(ok));else if(Date.now()>deadline)fail(Error('mount timeout'));else setTimeout(tick,50);};tick();})`);
const image=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await writeFile(`${out}/${phase}-${name}.png`,Buffer.from(image.data,'base64'));
if(name.startsWith('icons')){
const data=await evaluate(`window.auditMetrics()`);samples.push({theme:name,data});
for(const section of ['agent-icons','globe-icon']){const clip=await evaluate(`(()=>{const r=document.getElementById('${section}').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,scale:1};})()`);const crop=await call('Page.captureScreenshot',{format:'png',clip});await writeFile(`${out}/${phase}-${name}-${section}.png`,Buffer.from(crop.data,'base64'));}
}
}
await writeFile(`${out}/${phase}-metrics.json`,JSON.stringify(samples,null,2));
}finally{if(ws)ws.close();if(chrome)chrome.kill();await s.close();}
