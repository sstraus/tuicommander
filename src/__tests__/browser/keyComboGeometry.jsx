import { render } from 'solid-js/web';
import { KeyComboCapture } from '../../components/shared/KeyComboCapture';
import '../../global.css';
render(() => <div id="geometry"><KeyComboCapture value="Cmd+K" onChange={() => {}} /><KeyComboCapture value="" onChange={() => {}} /></div>, document.getElementById('root'));
// Mount one display and one capture field, using the actual capture transition.
document.querySelectorAll('#geometry button')[1].click();
requestAnimationFrame(() => { window.controlGeometryReady = true; });
