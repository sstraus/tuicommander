import { render } from 'solid-js/web';
import { HelpPanel } from '../../components/HelpPanel/HelpPanel';
import '../../global.css';
window.__APP_VERSION__='geometry-fixture';
window.__TAURI_INTERNALS__={invoke:async()=>undefined};
// Mount the actual main and legend views together to measure both header actions.
render(()=><><HelpPanel visible onClose={()=>{}}/><HelpPanel visible onClose={()=>{}}/></>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const overlays=[...document.querySelectorAll('[class*="overlay"]')];
 overlays[0].querySelectorAll('button').forEach(button=>{if(button.textContent.trim()==='UI Legend')button.click();});
 const geometry=document.createElement('section');geometry.id='geometry';document.body.append(geometry);
 for(const overlay of overlays){overlay.style.position='relative';overlay.style.inset='auto';geometry.append(overlay);}
 window.controlGeometryReady=true;
});
