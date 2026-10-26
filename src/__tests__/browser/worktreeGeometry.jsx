import { render } from 'solid-js/web';
import { CreateWorktreeDialog } from '../../components/CreateWorktreeDialog/CreateWorktreeDialog';
import '../../global.css';
render(()=><section id="geometry"><CreateWorktreeDialog visible={true} suggestedName="" existingBranches={['main']} worktreeBranches={['main']} worktreesDir="/fixture" baseRefs={[{name:'main',kind:'local',is_default:true},{name:'origin/main',kind:'remote',is_default:false}]} onGenerateName={async()=>'feature/name'} onClose={()=>{}} onCreate={()=>{}}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 document.querySelector('[class*="overlay"]').style.position='static';
 window.controlGeometryReady=true;
});

window.geometryIconLabels=["Generate random name"];
