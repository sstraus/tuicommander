import { render } from 'solid-js/web';
import { SmartPromptsDropdown } from '../../components/SmartPromptsDropdown/SmartPromptsDropdown';
import { smartPromptsDropdownStore } from '../../stores/smartPromptsDropdown';
import '../../global.css';
smartPromptsDropdownStore.open();
render(() => <section id="geometry" style="width:360px;height:400px;padding:16px;display:flex;justify-content:flex-end;align-items:flex-start;background:var(--bg-secondary)"><SmartPromptsDropdown onOpenSettings={() => {}} /></section>, document.getElementById('root'));
window.controlGeometryReady = true;
