import { createStore } from 'solid-js/store';
import { render } from 'solid-js/web';
import { RepositoryActions } from '../../components/SettingsPanel/RepositoryActions';
import { RepoWorktreeTab } from '../../components/SettingsPanel/tabs/RepoWorktreeTab';
import { repoDefaultsStore } from '../../stores/repoDefaults';
import '../../global.css';

// Layout and native input only: internal repository props, no save or external system.
window.addEventListener('error',event=>{window.controlGeometryError=event.message;});
window.addEventListener('unhandledrejection',event=>{window.controlGeometryError=String(event.reason);});
const [repo,setRepo]=createStore({
		path: "/workspace/team/application-with-a-long-repository-name",
		displayName: "Application workspace",
		color: "",
		autoConsolidateWorktrees: false,
		devServerUrl: null,
		branchLabels: {},
		baseBranch: null,
		copyIgnoredFiles: null,
		copyUntrackedFiles: false,
		setupScript: null,
		runScript: "pnpm dev",
		archiveScript: null,
		terminalMetaHotkeys: null,
		worktreeStorage: null,
		promptOnCreate: null,
		deleteBranchOnRemove: null,
		autoArchiveMerged: null,
		orphanCleanup: null,
		prMergeStrategy: null,
		afterMerge: null,
		autoFetchIntervalMinutes: null,
		autoDeleteOnPrClose: null,
		mcpUpstreams: null,
		prHideDrafts: null,
		prHideConflicting: null,
		prHideCiFailing: null,
	});
window.repositoryActions={exports:0,resets:0};
render(()=> <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)">
 <RepoWorktreeTab settings={repo} defaults={repoDefaultsStore.state} onUpdate={(key,value)=>setRepo(key,value)}/>
 <RepositoryActions path={repo.path} canExport onExport={()=>{window.repositoryActions.exports++;}} onReset={()=>{window.repositoryActions.resets++;}}/>
</section>,document.getElementById('root'));
window.controlGeometryReady=true;
