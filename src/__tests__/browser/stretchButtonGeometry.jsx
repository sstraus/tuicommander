import { render } from 'solid-js/web';
import { Button } from '../../components/ui/Button';
import '../../global.css';

// Catches fixed-height, rounded-edge, compact-override and undersized stretch actions.
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <div data-row="default" style="display:flex;gap:8px"><Button>Default</Button><Button disabled>Disabled</Button></div>
  {[20,72].map(height => <div data-stretch-case={height} style={{display:'flex',height:`${height}px`, 'margin-block':'24px'}}>
    <Button geometry="stretch" variant="ghost" aria-label="Scroll left">‹</Button>
    <Button geometry="stretch" size="compact" variant="danger" aria-label="Delete" disabled>×</Button>
  </div>)}
</section>, document.getElementById('root'));
requestAnimationFrame(() => { window.controlGeometryReady = true; });
