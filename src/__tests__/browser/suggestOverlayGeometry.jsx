import { render } from 'solid-js/web';
import SuggestOverlay from '../../components/SuggestOverlay/SuggestOverlay';
import '../../global.css';
render(() => <div style="height:160px;position:relative"><SuggestOverlay items={['Inspect the control mapping', 'Run the focused test cases', 'Review measured geometry', 'Finish the screenshot capture']} onSelect={() => {}} onDismiss={() => {}} /></div>, document.getElementById('root'));
document.querySelector('#root > div > div').id = 'geometry';
window.controlGeometryReady = true;
