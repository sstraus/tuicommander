import { render } from 'solid-js/web';
localStorage.clear();
const { SettingsScreen } = await import('../../mobile/screens/SettingsScreen');
import '../../mobile/mobile.css';
// Offline voice and unsupported push avoid device prompts; push gesture behavior has its own component test.
delete window.PushManager;
const chrome={bg_primary:'#1e1e1e',bg_secondary:'#252526',bg_tertiary:'#2d2d30',bg_highlight:'#37373d',fg_primary:'#cccccc',fg_secondary:'#aaaaaa',fg_muted:'#999999',accent:'#59a8dd',accent_hover:'#7abde5',border:'#444444',success:'#00aa00',warning:'#aaaa00',error:'#cc0000',text_on_accent:'#000000',text_on_error:'#ffffff',text_on_success:'#ffffff'};
window.fetch=async(input)=>{
 const url=new URL(typeof input==='string'?input:input.url,location.origin);
 if(url.pathname==='/config/ui-prefs')return Response.json({mobile_theme:'commander'});
 if(url.pathname==='/config/themes')return Response.json([{key:'commander',name:'Commander',terminal:{background:'#1e1e1e',foreground:'#cccccc',cursor:'#cccccc',ansi:[]},app_chrome:chrome}]);
 if(url.pathname==='/api/version')return Response.json({version:'fixture'});
 throw new Error(`Unexpected fixture RPC ${url.pathname}`);
};
render(() => <section id="geometry" style="height:1400px;background:var(--bg-secondary)"><SettingsScreen isConnected={false} /></section>,document.getElementById('root'));
for(let i=0;i<60&&document.querySelector('[aria-label="Theme"]').disabled;i++)await new Promise(resolve=>requestAnimationFrame(resolve));
const named=name=>document.querySelector(`[aria-label="${name}"]`);
for(const name of ['Autocorrect and spellcheck','New text indicator','Notification sounds','Keep screen awake while dictating']){
 const toggle=named(name);if(toggle.role!=='switch'||!toggle.checked)throw new Error(`Missing checked shared switch ${name}`);toggle.click();if(toggle.checked)throw new Error(`Switch did not toggle ${name}`);
}
for(let i=0;i<30;i++)named('Larger Terminal font').click();
if(!named('Larger Terminal font').disabled||document.querySelector('output[aria-label="Terminal font size"]').textContent!=='24 px')throw new Error('Terminal maximum changed');
for(let i=0;i<30;i++)named('Smaller Chat font').click();
if(!named('Smaller Chat font').disabled||document.querySelector('output[aria-label="Chat font size"]').textContent!=='10 px')throw new Error('Chat minimum changed');
if(localStorage.getItem('tuic-mobile-sounds')!=='false')throw new Error('Sound preference not saved');
if(document.querySelector('a[href="/process/monitor"]').target!=='_blank')throw new Error('Process link changed');
window.controlGeometryReady=true;
