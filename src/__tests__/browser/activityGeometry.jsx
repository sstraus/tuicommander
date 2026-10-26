import { render } from 'solid-js/web';
import { ActivityDashboard } from '../../components/ActivityDashboard/ActivityDashboard';
import { activityDashboardStore } from '../../stores/activityDashboard';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async()=>[]};
const row = (id, isPromoted) => ({id,name:id,project:null,agent:'ego',status:{label:'Idle',className:''},isWorking:false,lastDataAt:null,idleSince:null,lastPrompt:null,agentIntent:null,currentTask:null,activeSubTasks:0,isActive:false,isPromoted,subAgentTag:null});
activityDashboardStore.open();
render(()=><section id="geometry"><ActivityDashboard terminals={()=>[row('Available agent',false),row('Promoted agent',true)]} onPromote={()=>{}} onSelect={()=>{}}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const dashboard=document.querySelector('[title="Promote to Global Workspace"]').closest('[class*="dashboard"]');
 // Keep the real module mounted while clipping the fixture to its own dialog.
 const overlay=dashboard.parentElement;
 overlay.style.position='static'; overlay.style.paddingTop='0';
 window.controlGeometryReady=true;
});

window.geometryIconLabels=["Promote to Global Workspace", "Remove from Global Workspace"];
