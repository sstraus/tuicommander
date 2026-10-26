import { render } from 'solid-js/web';
import { NewConversationDialog } from '../../components/AIChatPanel/NewConversationDialog';
import '../../global.css';
// Real form; no ACP connection or agent launch.
render(()=><NewConversationDialog chat={{error:()=>'',startSession:async()=>{}}} onClose={()=>{}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{document.querySelector('[role="dialog"]').id='geometry';window.controlGeometryReady=true;});
