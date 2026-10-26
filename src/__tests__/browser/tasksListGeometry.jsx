import { render } from 'solid-js/web';
import { TasksList } from '../../components/Tasks/list/TasksList';
import { listDemo } from '../../components/Tasks/list/demo';
import '../../global.css';
const provider=listDemo(3);
render(() => <section id="geometry" style="height:600px"><TasksList providers={[provider]} onOpen={()=>{}}/></section>,document.getElementById('root'));
document.querySelector('[aria-expanded]').click();
window.controlGeometryReady=true;
