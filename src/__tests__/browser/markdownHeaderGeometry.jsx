import { render } from 'solid-js/web';
import { MarkdownTab } from '../../components/MarkdownTab/MarkdownTab';
import '../../global.css';
// Actual tab with a read-only in-memory file boundary; no disk writes or agent launch.
window.__TAURI_INTERNALS__={invoke:async command=>command==='read_file'?'# Geometry\n\nFixture text.\n':undefined};
render(()=><MarkdownTab tab={{id:'geometry-md',type:'file',repoPath:'/fixture',filePath:'review.md'}}/>,document.getElementById('root'));
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));
for(let i=0;i<180&&!document.querySelector('button[title="Edit file"]');i++)await frame();
const edit=document.querySelector('button[title="Edit file"]');
if(!edit)throw new Error('Markdown header did not mount');
edit.parentElement.id='geometry';
window.controlGeometryReady=true;
