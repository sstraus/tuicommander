import {render} from 'solid-js/web';
import '../../global.css';
import '../../styles.css';
const surface=new URLSearchParams(location.search).get('surface')??'terminal';
// In-memory backend replies expose real layout, not persistence or transport behavior.
window.__TAURI_INTERNALS__={invoke:async(command,args)=>{
 if(command==='story_capabilities')return true;
 if(command==='story_action_command')return args.action.action==='list_plans'?{type:'plans',value:[]}:{type:'plan_sources',value:[]};
 return undefined;
},transformCallback:()=>0};
const root=document.getElementById('root');
if(surface==='terminal'){
 const [{TabBar},{ViewModeToggle},{terminalsStore}]=await Promise.all([import('../../components/TabBar/TabBar'),import('../../components/Terminal/TerminalChatView'),import('../../stores/terminals')]);
 const id=terminalsStore.add({name:'Density example',sessionId:null,fontSize:14,cwd:null,agentType:'claude'});
 render(()=><section id="geometry" style="background:var(--bg-secondary);padding:16px;width:1000px"><div id="tab-neighbour"><TabBar onTabSelect={()=>{}} onTabClose={()=>{}} onCloseOthers={()=>{}} onCloseToRight={()=>{}} onNewTab={()=>{}}/></div><div style="position:relative;height:100px"><ViewModeToggle terminalId={id}/></div></section>,root);
}else if(surface==='session-dialog'){
 const [{SessionControls},{default:s}]=await Promise.all([import('../../components/AIChatPanel/SessionControls'),import('../../components/AIChatPanel/AIChatPanel.module.css')]);
 const done=async()=>{};
 const chat={sessionId:()=> 'density-chat',title:()=> 'Density conversation',phase:()=> 'live',busy:()=> false,held:()=> false,capabilities:()=> ({}),sessions:()=> [],configOptions:()=> [{id:'model',name:'Model',type:'select',currentValue:'fixture',options:[{value:'fixture',name:'Fixture model'}]}],operatingRepo:()=> '/density/example',pinnedRepo:()=> null,launch:()=> null,error:()=> null,clearError:()=> {},startSession:done,selectSession:done,setOption:done,toggleRepoPin:()=> {}};
 render(()=><section id="geometry" style="height:850px;background:var(--bg-secondary)"><div class={s.tidyHeader}><SessionControls chat={chat} onNewOptions={()=>{}}/></div></section>,root);
 document.querySelector('details').open=true;
 document.querySelector('[aria-label="Session settings"]').click();
}else if(surface==='variable-dialog'){
 const [{Toolbar},{VariableInputDialog}]=await Promise.all([import('../../components/Toolbar/Toolbar'),import('../../components/SmartPromptsDropdown/VariableInputDialog')]);
 render(()=><section id="geometry" style="height:850px;background:var(--bg-secondary)"><Toolbar repoPath="/density/example"/></section>,root);
 // Same DOM ownership as SmartPromptsDropdown: the real dialog is inside the app toolbar.
 render(()=><VariableInputDialog variables={['branch_name']} promptName="Density prompt" onSubmit={()=>{}} onCancel={()=>{}}/>,document.querySelector('#toolbar'));
}else if(surface==='stories'){
 const {StoriesDialog}=await import('../../components/StoriesDialog/StoriesDialog');
 render(()=><section id="geometry" style="width:1100px;height:650px"><StoriesDialog project="/density/example" mode="detached" onClose={()=>{}}/></section>,root);
}else if(surface==='settings'){
 const {GeneralTab}=await import('../../components/SettingsPanel/tabs/GeneralTab');
 render(()=><section id="geometry" style="width:900px;padding:16px;background:var(--bg-secondary)"><GeneralTab/></section>,root);
 // Keep the actual Power Management and Resource Limits cards, not the unrelated pages.
 for(const card of [...root.querySelectorAll('section')])if(card.id!=='geometry'&&card.querySelector('h3')&&!['Power Management','Resource Limits'].includes(card.querySelector('h3').textContent))card.remove();
 for(const header of root.querySelectorAll('h2'))header.parentElement.remove();
}else if(surface==='files'){
 const {FileBrowserPanel}=await import('../../components/FileBrowserPanel/FileBrowserPanel');
 render(()=><section id="geometry" style="width:760px;height:400px"><FileBrowserPanel visible repoPath={null} mode="detached" onClose={()=>{}} onFileOpen={()=>{}}/></section>,root);
}
await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
window.controlGeometryReady=true;

// Catches: missing toolbar opt-in, or inherited toolbar tokens shrinking Settings and nested dialogs.
window.controlGeometryRows=()=>{
 const root=document.querySelector('#geometry');
 const dialog=surface==='session-dialog'?root.querySelector('[role="dialog"]'):surface==='variable-dialog'?root.querySelector('form'):null;
 const controls=dialog?[...dialog.querySelectorAll('[data-form-control]')]:surface==='terminal'?[...root.querySelectorAll('#tab-neighbour [data-form-control], [role="tablist"]')]:[...root.querySelectorAll('[data-form-control]:not([type="checkbox"]):not([type="range"]), [role="tablist"]')];
 if(controls.length<2)throw new Error('Density fixture must mount real controls');
 return [{size:surface==='settings'||dialog?'default':'compact',heights:controls.map(element=>({tag:element.tagName,name:element.getAttribute('aria-label')||element.textContent?.trim(),height:element.getBoundingClientRect().height}))}];
};
