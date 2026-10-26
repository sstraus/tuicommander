import { render } from 'solid-js/web';
import { DiffTab } from '../../components/DiffTab/DiffTab';
import { uiStore } from '../../stores/ui';
import '../../global.css';
// Synthetic host reply; the actual diff renderer and selection/comment controls are mounted.
const diff='diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -10,3 +10,3 @@\n context\n-old\n+new\n tail\n';
window.__TAURI_INTERNALS__={invoke:async command=>command==='get_file_diff'?diff:undefined};
uiStore.setDiffViewMode('unified');
render(()=><section id="geometry" style={{height:'600px'}}><DiffTab tabId="geometry-diff" repoPath="/fixture" filePath="file.txt"/></section>,document.getElementById('root'));
await new Promise((resolve,reject)=>{
 const stop=setTimeout(()=>reject(new Error('Real diff rows did not mount')),10000);
 const check=()=>{
  const row=[...document.querySelectorAll('tr')].find(row=>row.querySelector('[data-line-new-num]')&&!row.querySelector('[data-line-old-num]'));
  if(!row)return requestAnimationFrame(check);
  row.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,button:0}));
  document.dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));
  const comment=document.querySelector('button[title="Comment on selected lines"]');
  if(!comment)return requestAnimationFrame(check);
  comment.click();clearTimeout(stop);resolve();
 };requestAnimationFrame(check);
});
await Promise.all(document.getAnimations().map(animation=>animation.finished));
window.controlGeometryReady=true;
