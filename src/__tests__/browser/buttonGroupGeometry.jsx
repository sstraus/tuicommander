import { render } from 'solid-js/web';
import { Button } from '../../components/ui/Button';
import { ButtonGroup } from '../../components/ui/ButtonGroup';
import '../../global.css';
// Catches separated or rounded interior edges, double dividers and undersized compact touch actions.
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
 <div data-row="default"><Button>Default</Button><Button disabled>Disabled</Button></div>
 {['default','compact'].map(size => <div style="margin-block:16px">
   <ButtonGroup data-joined-case={size} aria-label={`${size} actions`}><Button size={size}>Run</Button><Button size={size} disabled>Middle</Button><Button size={size}>Choose</Button></ButtonGroup>
 </div>)}
 <ButtonGroup data-joined-case="default" aria-label="Single action"><Button>Single</Button></ButtonGroup>
</section>, document.getElementById('root'));
requestAnimationFrame(() => { window.controlGeometryReady = true; });
