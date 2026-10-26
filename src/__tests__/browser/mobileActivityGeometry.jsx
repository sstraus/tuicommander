import { render } from 'solid-js/web';
import { ActivityItem } from '../../mobile/components/ActivityItem';
import { Button, Select } from '../../components/ui';
import '../../mobile/mobile.css';
const item = {id:'geometry',pluginId:'core',sectionId:'messages',title:'Agent completed work',subtitle:'ran for 2343s · 1 blocks',icon:'<svg/>',createdAt:1000,dismissible:true};
let tapped;
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <ActivityItem item={item} onTap={value => {tapped=value;}}/>
  <ActivityItem item={{...item,title:'A long activity title '.repeat(12),subtitle:undefined}} onTap={()=>{}}/>
  <div data-row="default" style="display:flex;gap:8px"><Button>Reference action</Button><Select aria-label="Reference status"><option>Ready</option></Select></div>
</section>, document.getElementById('root'));
const row=document.querySelector('button[data-control-layout="row"]');
if (!row || row.type!=='button') throw new Error('Activity row lost native action semantics');
row.click();
if(tapped!==item || !row.textContent.includes('ran for 39 min · 1 block')) throw new Error('Activity tap or subtitle changed');
if([...document.querySelectorAll('button[data-control-layout="row"]')].some(row=>row.getBoundingClientRect().height<44)) throw new Error('Activity row touch target below 44px');
window.controlGeometryReady=true;
