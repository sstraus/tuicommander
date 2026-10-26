import { render } from 'solid-js/web';
import '../../global.css';
// Internal layout replies only; no token, Telegram service or poller is used.
window.addEventListener('error',event=>{window.controlGeometryError=event.message;});
window.addEventListener('unhandledrejection',event=>{window.controlGeometryError=String(event.reason);});
window.telegramReads=0;
window.fetch=async (input,init)=>{
 window.telegramReads++;
 const path=new URL(typeof input==='string'?input:input.url,location.origin).pathname;
 if(path!=='/config/telegram' || (init?.method??'GET')!=='GET')throw new Error(`Unexpected geometry request: ${path}`);
 return new Response(JSON.stringify({enabled:true,token_set:true,bot_alias:'Fixture bot',registered_agent_name:'Fixture agent',chats:['123456'],pairing_pending:false,recent_events:[],connected:true,polling_owner:'this_app',last_error:null,last_message_time:null}),{headers:{'content-type':'application/json'}});
};
const { TelegramTab }=await import('../../components/SettingsPanel/tabs/TelegramTab');
render(()=> <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)"><TelegramTab /></section>,document.getElementById('root'));
const ready=()=>{
 if(document.body.textContent.includes('Fixture agent')){window.controlGeometryReady=true;observer.disconnect();}
};
const observer=new MutationObserver(ready);
observer.observe(document.getElementById('root'),{childList:true,subtree:true,characterData:true});
ready();
