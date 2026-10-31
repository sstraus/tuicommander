import {render} from 'solid-js/web';
import {For} from 'solid-js';
import {AgentIcon} from '../../components/ui/AgentIcon';
import {GlobeIcon} from '../../components/GlobeIcon';
import '../../global.css';
import commander from '../../../src-tauri/src/themes/commander.json';
import paper from '../../../src-tauri/src/themes/vscode-light.json';
const light=new URLSearchParams(location.search).get('theme')==='light';
const palette=light?paper.appChrome:commander.appChrome;
document.documentElement.style.setProperty('--fg-primary',palette.foreground);
document.documentElement.style.setProperty('--bg-primary',palette.background);
document.documentElement.style.colorScheme=light?'light':'dark';
const agents=['claude','gemini','codex','amp','cursor','opencode','droid'];
render(()=><main style="padding:24px;background:var(--bg-primary);color:var(--fg-primary)">
<h1>Shared production icon audit</h1>
<section id="agent-icons"><h2>Agent identity — same requested size</h2><For each={[14,16,22,64]}>{size=><div style="display:flex;gap:24px;align-items:center;padding:12px"><span style="width:45px">{size}px</span><For each={agents}>{agent=><div data-agent={agent} data-size={size} style="width:110px;display:flex;align-items:center;gap:8px"><AgentIcon agent={agent} size={size}/><span>{agent}</span></div>}</For></div>}</For></section>
<section id="globe-icon"><h2>Remote globe — production component</h2><div style="display:flex;gap:32px;align-items:center;padding:16px"><For each={[14,22,96]}>{size=><GlobeIcon size={size}/>}</For></div></section>
</main>,document.getElementById('root'));
window.auditMetrics=()=>({foreground:getComputedStyle(document.querySelector('main')).color,agents:[...document.querySelectorAll('[data-agent]')].map(el=>{const svg=el.querySelector('svg'),p=svg.querySelector('path'),b=p.getBBox(),v=svg.viewBox.baseVal;return {agent:el.dataset.agent,size:Number(el.dataset.size),fill:getComputedStyle(svg).fill,inside:b.x>=v.x-.01&&b.y>=v.y-.01&&b.x+b.width<=v.x+v.width+.01&&b.y+b.height<=v.y+v.height+.01,paintWidth:b.width/v.width*svg.clientWidth,paintHeight:b.height/v.height*svg.clientHeight,bounds:{x:b.x,y:b.y,width:b.width,height:b.height}};}),globe:[...document.querySelectorAll('#globe-icon svg')].map(svg=>{const paths=[...svg.querySelectorAll('path')];const filled=(x,y)=>paths.some(p=>p.isPointInFill(new DOMPoint(x,y)));return {size:svg.clientWidth,height:svg.clientHeight,openings:[[3,5],[7,5],[9,5],[13,5],[3,11],[7,11],[9,11],[13,11]].map(([x,y])=>({x,y,filled:filled(x,y)})),structure:[[8,.75],[8,15.25],[.75,8],[15.25,8],[4,8],[12,8],[4.9,5],[11.1,5],[4.9,11],[11.1,11]].map(([x,y])=>({x,y,filled:filled(x,y)}))};})});
window.auditReady=true;
