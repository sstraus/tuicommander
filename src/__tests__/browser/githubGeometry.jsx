import { render } from 'solid-js/web';
import '../../global.css';

// Layout-only internal responses. No request reaches a backend, account or credential store.
const responses = {
 '/github/auth/status': {authenticated:false,login:null,avatar_url:null,source:'none',scopes:null},
 '/github/diagnostics': {circuit_breaker_open:false,circuit_breaker_status:'OK',repos_not_found:[],repos_monitored:0},
 '/github/accounts': [{id:'fixture',host:{host:'ghe.example.test'},login:'Fixture account',kind:'ghe_pat'}],
 '/circleci/token': {configured:true,source:'vault'},
};
window.fetch = async (input, init) => {
 const path=new URL(typeof input==='string'?input:input.url,location.origin).pathname;
 if((init?.method ?? 'GET')!=='GET' || !(path in responses))throw new Error(`Unexpected geometry request: ${path}`);
 return new Response(JSON.stringify(responses[path]),{headers:{'content-type':'application/json'}});
};
const { GitHubTab } = await import('../../components/SettingsPanel/tabs/GitHubTab');
render(() => <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)"><GitHubTab /></section>,document.getElementById('root'));
const ready=()=>{
 if(document.body.textContent.includes('Fixture account') && document.body.textContent.includes('Configured (vault)')) {
  window.controlGeometryReady=true;
  observer.disconnect();
 }
};
const observer=new MutationObserver(ready);
observer.observe(document.getElementById('root'),{childList:true,subtree:true,characterData:true});
ready();
