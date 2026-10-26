import { EditorView } from '@codemirror/view';
import { render } from 'solid-js/web';
import { LiveMarkdownEditor } from '../../components/MarkdownTab/LiveMarkdownEditor';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async()=>undefined};
// Real editors with in-memory disk/save ports; no filesystem write is performed.
render(()=><section id="geometry"><div id="composer"><LiveMarkdownEditor content="hello world" onSave={async()=>true} readDisk={async()=> 'hello world'}/></div><div id="conflict"><LiveMarkdownEditor content="second block" onSave={async()=>true} readDisk={async()=> 'external edit'}/></div></section>,document.getElementById('root'));
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));
async function waitFor(selector,root=document){for(let i=0;i<180;i++){const element=root.querySelector(selector);if(element)return element;await frame();}throw new Error('Missing fixture element '+selector);}
for(const id of ['composer','conflict']){
 const root=document.getElementById(id);
 (await waitFor('[data-comment-source-start]',root)).click();
 const view=EditorView.findFromDOM(await waitFor('.cm-editor',root));
 view.dispatch({changes:{from:0,insert:'Edited '},selection:{anchor:0,head:5}});
 await frame();
 const buttons=()=>[...root.querySelectorAll('button')];
 if(id==='composer')buttons().find(button=>button.textContent==='Comment').click();
 else buttons().find(button=>button.textContent==='Save').click();
 await frame();await frame();
}
await waitFor('input',document.getElementById('composer'));
await waitFor('[role="alert"]',document.getElementById('conflict'));
window.controlGeometryReady=true;
