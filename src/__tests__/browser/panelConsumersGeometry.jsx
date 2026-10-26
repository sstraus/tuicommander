import { render } from 'solid-js/web';
import { OutlinePanel } from '../../components/OutlinePanel/OutlinePanel';
import { ReferencesPanel } from '../../components/ReferencesPanel/ReferencesPanel';
import '../../global.css';
// Empty local stores: no file read or backend search starts.
render(()=><div id="geometry" style={{display:'flex','flex-wrap':'wrap',gap:'16px'}}>
 <OutlinePanel visible onClose={()=>{}}/>
 <ReferencesPanel visible onClose={()=>{}}/>
</div>,document.getElementById('root'));
requestAnimationFrame(()=>{window.controlGeometryReady=true;});
