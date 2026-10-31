import {render} from 'solid-js/web';
import {For} from 'solid-js';
import {AgentIcon} from '../../components/ui/AgentIcon';
import {GlobeIcon} from '../../components/GlobeIcon';
import '../../global.css';
const agents=['claude','gemini','codex','amp','cursor','opencode','droid'];
render(()=><main style="padding:24px;background:var(--bg-primary);color:var(--fg-primary)">
<h1>Shared production icon audit</h1>
<section id="agent-icons"><h2>Agent identity — same requested size</h2><For each={[14,16,22,64]}>{size=><div style="display:flex;gap:24px;align-items:center;padding:12px"><span style="width:45px">{size}px</span><For each={agents}>{agent=><div data-agent={agent} data-size={size} style="width:110px;display:flex;align-items:center;gap:8px"><AgentIcon agent={agent} size={size}/><span>{agent}</span></div>}</For></div>}</For></section>
<section id="globe-icon"><h2>Remote globe — production component</h2><div style="display:flex;gap:32px;align-items:center;padding:16px"><For each={[14,22,96]}>{size=><GlobeIcon size={size}/>}</For></div></section>
</main>,document.getElementById('root'));
window.auditMetrics=()=>({agents:[...document.querySelectorAll('[data-agent]')].map(el=>{const svg=el.querySelector('svg'),p=svg.querySelector('path'),b=p.getBBox(),v=svg.viewBox.baseVal;return {agent:el.dataset.agent,size:Number(el.dataset.size),paintWidth:b.width/v.width*svg.clientWidth,paintHeight:b.height/v.height*svg.clientHeight,bounds:{x:b.x,y:b.y,width:b.width,height:b.height}};}),globe:[...document.querySelectorAll('#globe-icon svg')].map(svg=>{const p=svg.querySelector('path');return {size:svg.clientWidth,points:[[6,5],[10,5],[6,11],[10,11],[8,8],[1,8]].map(([x,y])=>({x,y,filled:p.isPointInFill(new DOMPoint(x,y))}))};})});
window.auditReady=true;
