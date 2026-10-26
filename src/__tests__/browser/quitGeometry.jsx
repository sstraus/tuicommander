import { render } from 'solid-js/web';
import { CloseDialog } from '../../components/QuitDialog/CloseDialog';
import { closeDialogStore } from '../../stores/closeDialog';
import '../../global.css';
// Local presentation only; never resolve or invoke a real app quit.
closeDialogStore.present({kind:'ask',sessions:2});
render(()=><CloseDialog/>,document.getElementById('root'));
requestAnimationFrame(()=>{document.querySelector('[role="dialog"]').id='geometry';window.controlGeometryReady=true;});
