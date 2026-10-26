import { render } from 'solid-js/web';
import { VariableInputDialog } from '../../components/SmartPromptsDropdown/VariableInputDialog';
import '../../global.css';
render(() => <VariableInputDialog variables={['goal', 'scope']} suggestions={{goal:'Review changes'}} promptName="Review changes" onSubmit={() => {}} onCancel={() => {}} />, document.getElementById('root'));
document.querySelector('form').id = 'geometry';
window.controlGeometryReady = true;
