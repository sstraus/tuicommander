import { render } from 'solid-js/web';
import { SlashMenuOverlay } from '../../mobile/components/SlashMenuOverlay';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let selected=[],closed=0;
render(() => <section id="geometry" style="height:480px;padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <div style="position:relative;top:400px"><SlashMenuOverlay sessionId="geometry" items={[
    {command:'/review',description:'Review the current change',highlighted:true},
    {command:'/old',description:'(removed)',highlighted:false},
    {command:'/help',description:'A long description that must wrap without narrowing the command navigation targets',highlighted:false}
  ]} onSelect={value=>selected.push(value)} onClose={()=>{closed++;}}/></div>
</section>,document.getElementById('root'));
const rows=[...document.querySelectorAll('button[data-control-layout="row"]')];
if(rows.length!==2)throw new Error('Removed command returned');
rows[0].click();
if(selected.join()!=='/review')throw new Error('Command selection changed');
document.querySelector('[aria-label="Close slash menu"]').click();
if(closed!==1)throw new Error('Dismiss callback changed');
for(const row of rows)if(row.getBoundingClientRect().height<44)throw new Error('Command row lost target');
window.controlGeometryReady=true;
