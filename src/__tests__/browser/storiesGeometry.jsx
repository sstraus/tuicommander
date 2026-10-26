import { render } from 'solid-js/web';
import { StoryEditForm } from '../../components/StoriesDialog/StoryEditForm';
import '../../global.css';

render(() => <section id="geometry" style="width:min(600px,100%);padding:16px;background:var(--bg-secondary)"><StoryEditForm
 story={{id:'story',revision:1,title:'Geometry check',priority:2,storyType:'feature',origin:{type:'native'},criteria:['A shared control row'],fileScope:[],dependencies:[]}}
 candidates={[]} busy={false} error="" onSave={()=>{}} onCancel={()=>{}}/>
</section>,document.getElementById('root'));
window.controlGeometryReady = true;
