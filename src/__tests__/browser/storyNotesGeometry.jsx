import { render } from 'solid-js/web';
import { StoryNotes } from '../../components/StoriesDialog/StoryNotes';
import '../../global.css';

// Presentation-only service boundary: no persisted notes or backend are under test.
render(() => <section id="geometry" style="width:min(600px,100%);padding:16px;background:var(--bg-secondary)"><StoryNotes project="/geometry" storyId="story" revision={1} busy={false}/></section>, document.getElementById('root'));
window.controlGeometryReady = true;
