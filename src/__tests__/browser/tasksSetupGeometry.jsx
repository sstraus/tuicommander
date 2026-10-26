import { render } from 'solid-js/web';
import { TasksSetup } from '../../components/Tasks/setup/TasksSetup';
import { pluginStore } from '../../stores/pluginStore';
import '../../global.css';
pluginStore.registerPlugin('github-work',{loaded:true});
render(() => <section id="geometry"><TasksSetup pluginId="github-work"/></section>,document.getElementById('root'));
while (document.querySelector('button:disabled')) await new Promise(resolve=>requestAnimationFrame(resolve));
window.controlGeometryReady=true;
