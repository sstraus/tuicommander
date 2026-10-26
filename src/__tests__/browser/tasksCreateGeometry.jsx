import { render } from 'solid-js/web';
import { TasksCreate } from '../../components/Tasks/create/TasksCreate';
import { provider } from '../../components/Tasks/detail/__tests__/detailFixture';
import '../../global.css';
render(() => <section id="geometry"><TasksCreate provider={provider()} onCreated={()=>{}} onCancel={()=>{}}/></section>,document.getElementById('root'));
while (document.querySelector('[role="status"]')) await new Promise(resolve=>requestAnimationFrame(resolve));
window.controlGeometryReady=true;
