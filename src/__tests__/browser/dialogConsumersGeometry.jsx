import { render } from 'solid-js/web';
import { ConfirmDialog } from '../../components/ConfirmDialog/ConfirmDialog';
import { CreateBranchDialog } from '../../components/CreateBranchDialog/CreateBranchDialog';
import { RenameBranchDialog } from '../../components/RenameBranchDialog/RenameBranchDialog';
import { PromptDialog } from '../../components/PromptDialog/PromptDialog';
import { RunCommandDialog } from '../../components/RunCommandDialog/RunCommandDialog';
import { WhatsNewDialog } from '../../components/WhatsNewDialog/WhatsNewDialog';
import { PostMergeCleanupDialog } from '../../components/PostMergeCleanupDialog/PostMergeCleanupDialog';
import { UpdateProgressDialog } from '../../components/UpdateProgressDialog/UpdateProgressDialog';
import { PromptInspector } from '../../components/PromptInspector/PromptInspector';
import { TunnelEditorModal } from '../../components/TunnelsPanel/TunnelEditorModal';
import { updaterStore } from '../../stores/updater';
import '../../global.css';
// Synthetic frontend IPC responses only. No git, SSH, updater, or receipt service is contacted.
window.__TAURI_INTERNALS__={invoke:async command=>{
 if(command==='list_ssh_config_hosts')return [];
 if(command==='list_ssh_agent_keys')return {keys:[],agent_type:''};
 if(command==='get_prompt_receipt')return {sections:[],captureLimited:false};
 if(command==='plugin:app|version')return '0.0.0';
 if(command.startsWith('plugin:updater') || command==='check_update_channel')throw new Error('Fixture update failure');
 return null;
}};
const noop=()=>{};
const style=document.createElement('style');
// Arrange actual dialogs side by side without overlapping fixed overlays.
style.textContent='#geometry {display:flex;flex-wrap:wrap;gap:16px;align-items:flex-start} #geometry > div {position:static;inset:auto;width:auto;background:transparent}';
document.head.append(style);
render(()=><div id="geometry">
 <ConfirmDialog visible title="Confirm fixture" message="Proceed?" discardLabel="Discard" onClose={noop} onConfirm={noop}/>
 <CreateBranchDialog visible onClose={noop} onCreate={async()=>{}}/>
 <RenameBranchDialog visible currentName="fixture" onClose={noop} onRename={async()=>{}}/>
 <PromptDialog visible title="Prompt fixture" defaultValue="Draft" onClose={noop} onConfirm={noop}/>
 <RunCommandDialog visible savedCommand="echo fixture" onClose={noop} onSaveAndRun={noop}/>
 <WhatsNewDialog visible version="fixture" highlights={[]} contributions={[]} onClose={noop}/>
 <PostMergeCleanupDialog branchName="fixture" baseBranch="main" repoPath="/fixture" isOnBaseBranch isDefaultBranch={false} hasTerminals={false} onExecute={noop} onSkip={noop}/>
 <UpdateProgressDialog/>
 <PromptInspector sessionId="fixture" onClose={noop}/>
 <TunnelEditorModal onClose={noop}/>
</div>,document.getElementById('root'));
await updaterStore.checkForUpdate();
requestAnimationFrame(()=>{window.controlGeometryReady=true;});
