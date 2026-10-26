import { render } from 'solid-js/web';
import { PromptDrawer } from '../../components/PromptDrawer/PromptDrawer';
import { promptLibraryStore } from '../../stores/promptLibrary';
import '../../global.css';
// Frontend-only fixture: no real persistence or terminal injection.
window.__TAURI_INTERNALS__={invoke:async()=>null};
promptLibraryStore.createPrompt({name:'Geometry prompt',content:'Review the changes',category:'custom',isFavorite:false});
promptLibraryStore.openDrawer();
render(()=><PromptDrawer/>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const editor=location.pathname.includes('Editor');
 if(editor)[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='+ New Prompt').click();
 requestAnimationFrame(()=>{
  document.querySelector(editor?'[class*="editorOverlay"] > div':'[class*="drawer"]').id='geometry';
  window.controlGeometryReady=true;
 });
});
