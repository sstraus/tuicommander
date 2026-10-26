import { render } from 'solid-js/web';
import { ComposePanel } from '../../components/ComposePanel/ComposePanel';
import '../../global.css';
render(()=><section id="geometry"><ComposePanel isOpen={()=>true} initialText={()=>'Draft'} onClose={()=>{}} onSend={()=>{}} onEnqueue={()=>{}} canEnqueue={()=>true} queuedCount={()=>1} onClearQueue={()=>{}} onLoadQueue={async()=>[{id:1,text:'Queued command',kind:'user_command'}]} onRemoveQueued={()=>{}} pinned={()=>true} onTogglePin={()=>{}} onDismiss={()=>{}} focusRequest={()=>0}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const badge=document.querySelector('[aria-expanded]');
 badge.click();
 const ready=()=>{
  if(!document.querySelector('[title="Remove from queue"]')){requestAnimationFrame(ready);return;}
  window.controlGeometryReady=true;
 };
 requestAnimationFrame(ready);
});

window.geometryIconLabels=["Remove from queue","Unpin from the terminal bottom","Queue for the next idle moment (Shift+Ctrl+Enter)","Attach images","Send (Ctrl+Enter)","Close compose panel"];
