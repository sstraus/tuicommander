import { render } from 'solid-js/web';
import { ContextMenu } from '../../components/ContextMenu/ContextMenu';
import '../../global.css';
render(()=><section id="geometry" style="height:220px"><ContextMenu x={16} y={16} visible={true} onClose={()=>{}} items={[{label:'Copy',shortcut:'⌘C',action:()=>{}},{label:'Paste',action:()=>{}},{label:'Unavailable',disabled:true,action:()=>{}}]}/></section>,document.getElementById('root'));
requestAnimationFrame(async()=>{
 await Promise.all(document.getAnimations().map(animation=>animation.finished));
 window.controlGeometryReady=true;
});
