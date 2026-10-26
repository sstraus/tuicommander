import { render } from 'solid-js/web';
import { CodeEditorTab } from '../../components/CodeEditorPanel/CodeEditorTab';
import { HtmlPreviewTab } from '../../components/HtmlPreviewTab/HtmlPreviewTab';
import '../../global.css';
// Synthetic read-only file replies: no disk writes or external application launch.
window.__TAURI_INTERNALS__={invoke:async command=>{
 if(command==='read_editor_file'||command==='read_external_file'||command==='read_repo_file')return 'Fixture text';
 if(command==='stat_path')return {exists:true,modified_at:1,size:12};
 if(command==='mdkb_outline'||command==='get_gutter_changes'||command==='get_file_blame')return [];
 return null;
}};
render(()=><div id="geometry">
 <div style={{height:'240px'}}><CodeEditorTab id="geometry-editor" repoPath="/fixture" filePath="notes.txt"/></div>
 <div style={{height:'180px'}}><HtmlPreviewTab tab={{id:'geometry-preview',type:'html-preview',repoPath:'/fixture',filePath:'/fixture/preview.txt',fileName:'preview.txt'}}/></div>
</div>,document.getElementById('root'));
requestAnimationFrame(()=>{window.controlGeometryReady=true;});
