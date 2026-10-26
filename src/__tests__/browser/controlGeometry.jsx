import { render } from 'solid-js/web';
import * as ui from '../../components/ui';
import '../../global.css';

// Before Button exists, mount the native action it replaces to expose padding-driven drift.
const Action = ui.Button ?? ((props) => <button {...props}/>);
render(() => <section id="geometry" style="display:flex;flex-direction:column;gap:12px;padding:16px;background:var(--bg-secondary)">
  <div data-row="default" style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
    {['primary', 'secondary', 'ghost', 'danger'].map(variant => <Action variant={variant}>{variant}</Action>)}
    <ui.Select aria-label="Status"><option>Ready</option></ui.Select>
    <ui.SegmentedControl aria-label="View" options={[{id:'list',label:'List'},{id:'board',label:'Board'}]} value="list" onChange={() => {}}/>
  </div>
  <div data-row="compact" style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
    <Action size="compact">Compact</Action><ui.Select compact aria-label="Compact status"><option>Ready</option></ui.Select>
  </div>
</section>, document.getElementById('root'));
window.controlGeometryReady = true;
