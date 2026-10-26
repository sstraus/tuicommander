import { render } from 'solid-js/web';
import { PrDiffTab } from '../../components/PrDiffTab/PrDiffTab';
import '../../global.css';
const diff='diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -1 +1 @@\n-old\n+new';
render(()=><section style="width:min(700px,100%);background:var(--bg-secondary)"><PrDiffTab prNumber={1} prTitle="Geometry" diff={diff}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const button=document.querySelector('button[title="Side-by-side"]');
 if(!button)throw new Error('Diff mode controls did not mount');
 const row=button.parentElement;row.dataset.row='compact';
 const header=row.parentElement;header.id='geometry';
 const bounds=header.getBoundingClientRect();
 const controls=[...row.querySelectorAll('button')].map(control=>control.getBoundingClientRect());
 // Catches: touch targets protrude beyond a fixed-height desktop header.
 if(controls.some(control=>control.top<bounds.top || control.bottom>bounds.bottom)) {
  window.controlGeometryError=JSON.stringify({bug:'PR diff header clips mode targets',width:innerWidth,header:{top:bounds.top,bottom:bounds.bottom,height:bounds.height},controls:controls.map(control=>({top:control.top,bottom:control.bottom,height:control.height}))});
  return;
 }
 window.controlGeometryReady=true;
});
