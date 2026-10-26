import { render } from 'solid-js/web';
import { TaskQueuePanel } from '../../components/TaskQueuePanel/TaskQueuePanel';
import { tasksStore } from '../../stores/tasks';
import '../../global.css';
// Local task store only: no process launched.
const running=tasksStore.create({name:'Running fixture',agentType:'claude'});tasksStore.start(running,'fixture');
tasksStore.create({name:'Pending fixture',agentType:'claude'});
render(()=><TaskQueuePanel visible onClose={()=>{}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{document.querySelector('[class*="panel"]').id='geometry';window.controlGeometryReady=true;});
