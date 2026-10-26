import { render } from 'solid-js/web';
import { TasksRoadmap } from '../../components/Tasks/roadmap/TasksRoadmap';
import { roadmapDemo } from '../../components/Tasks/roadmap/demo';
import '../../global.css';
const provider=roadmapDemo(3);
render(() => <section id="geometry" style="height:500px"><TasksRoadmap provider={provider} onOpen={()=>{}} initialDate={new Date(2026,9,1)}/></section>,document.getElementById('root'));
window.controlGeometryReady=true;
