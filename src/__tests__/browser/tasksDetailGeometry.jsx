import { render } from 'solid-js/web';
import { TasksDetail } from '../../components/Tasks/detail/TasksDetail';
import { detail, provider, reference } from '../../components/Tasks/detail/__tests__/detailFixture';
import '../../global.css';
const value=detail();
value.comments={entries:[],nextCursor:null};
render(() => <section id="geometry" style="height:880px"><TasksDetail provider={provider({detail:async()=>value})} reference={reference} onClose={()=>{}}/></section>,document.getElementById('root'));
while (!document.querySelector('[aria-label="Save Priority"]')) await new Promise(resolve=>requestAnimationFrame(resolve));
window.controlGeometryReady=true;
