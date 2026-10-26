import { render } from 'solid-js/web';
import { WorktreeManager } from '../../components/WorktreeManager/WorktreeManager';
import { worktreeManagerStore } from '../../stores/worktreeManager';
import { repositoriesStore } from '../../stores/repositories';
import '../../global.css';
// Local stores and synthetic orphan read; no worktree operations or real persistence.
window.__TAURI_INTERNALS__={invoke:async command=>command==='detect_orphan_worktrees'?['/fixture/orphan']:null};
for(const path of ['/fixture/a','/fixture/b']){repositoriesStore.add({path,displayName:path.endsWith('a')?'Alpha':'Beta'});repositoriesStore.setWorkspace(path,'feat',{worktreePath:path+'/feat'});}
worktreeManagerStore.open();worktreeManagerStore.toggleSelect('/fixture/a::feat');
render(()=><WorktreeManager actions={{onOpenTerminal:()=>{},onDelete:()=>{},onMergeAndArchive:()=>{}}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{document.querySelector('[class*="panel"]').id='geometry';window.controlGeometryReady=true;});
