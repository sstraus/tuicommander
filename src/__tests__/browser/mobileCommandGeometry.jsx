import { render } from 'solid-js/web';
import { CommandWidget } from '../../mobile/components/CommandWidget';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
render(() => <section id="geometry" style="height:900px;padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <CommandWidget sessionId="geometry-only" agentType="claude" onDismiss={()=>{}}/>
</section>, document.getElementById('root'));
window.controlGeometryReady=true;
