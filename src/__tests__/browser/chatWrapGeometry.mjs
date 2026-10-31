import {spawn} from 'node:child_process';
import {mkdir,writeFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {homedir} from 'node:os';
import {createServer} from 'vite';
import solid from 'vite-plugin-solid';
const directory=resolve(process.env.TUIC_CHAT_WRAP_DIR);
const phase=process.env.TUIC_CHAT_WRAP_PHASE??'after';
await mkdir(directory,{recursive:true});
const server=await createServer({configFile:false,plugins:[solid()],cacheDir:'.tmp/chat-wrap-vite',logLevel:'error',define:{__APP_VERSION__:JSON.stringify('geometry'),__BUILD_GIT_HASH__:JSON.stringify('geometry')},server:{host:'127.0.0.1',port:0}});
let chrome,socket;
try {
 await server.listen();
 const binary=join(homedir(),'.agent-browser/browsers/chrome-149.0.7827.54/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
 chrome=spawn(binary,['--headless=new','--disable-gpu','--no-first-run','--remote-debugging-port=0',`--user-data-dir=${directory}/cdp-profile`,'about:blank'],{stdio:['ignore','ignore','pipe']});
 const endpoint=await new Promise((ok,fail)=>{let stderr='';chrome.stderr.on('data',data=>{stderr+=data;const match=stderr.match(/DevTools listening on (ws:\/\/\S+)/);if(match)ok(match[1]);});chrome.once('error',fail);chrome.once('exit',code=>fail(new Error(`Chrome exited ${code}: ${stderr}`)));});
 socket=new WebSocket(endpoint);
 await new Promise((ok,fail)=>{socket.addEventListener('open',ok,{once:true});socket.addEventListener('error',fail,{once:true});});
 let sequence=0;const pending=new Map();
 socket.addEventListener('message',event=>{const message=JSON.parse(event.data);const request=pending.get(message.id);if(request){clearTimeout(request.timer);pending.delete(message.id);message.error?request.reject(new Error(JSON.stringify(message.error))):request.resolve(message.result);}});
 const send=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++sequence;const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`CDP ${method} did not respond`));},20000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
 const {targetId}=await send('Target.createTarget',{url:'about:blank'});
 const {sessionId}=await send('Target.attachToTarget',{targetId,flatten:true});
 const evaluate=async expression=>{const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true},sessionId);if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;};
 const samples=[];
 for(const width of [420,960]){
  await send('Emulation.setDeviceMetricsOverride',{width:1400,height:1000,deviceScaleFactor:1,mobile:false},sessionId);
  await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/src/__tests__/browser/chatWrapGeometry.html?width=${width}`},sessionId);
  await evaluate(`new Promise((ok,fail)=>{const stop=setTimeout(()=>fail(new Error('Real Transcript did not publish measurements')),20000);const check=()=>{const result=document.querySelector('#geometry-result');if(result){clearTimeout(stop);ok();}else requestAnimationFrame(check);};check();})`);
  samples.push(await evaluate(`JSON.parse(document.querySelector('#geometry-result').textContent)`));
  const screenshot=await send('Page.captureScreenshot',{format:'png'},sessionId);
  await writeFile(join(directory,`${phase}-${width}.png`),Buffer.from(screenshot.data,'base64'));
 }
 await writeFile(join(directory,`${phase}.json`),JSON.stringify(samples,null,2));
} finally {
 socket?.close();
 if(chrome?.pid && chrome.exitCode===null){const exited=new Promise(ok=>chrome.once('exit',ok));chrome.kill('SIGTERM');await exited;}
 await server.close();
}
