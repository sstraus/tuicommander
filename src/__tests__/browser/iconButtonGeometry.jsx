import { render } from 'solid-js/web';
import { Button } from '../../components/ui/Button';
import '../../global.css';

window.iconClicks=0;
const click=()=>window.iconClicks++;
render(() => <section id="geometry" style="display:flex;gap:12px;padding:16px;background:var(--bg-secondary);--accent:rgb(10,20,200);--success:rgb(10,200,20);--error:rgb(200,10,20)">
 <Button shape="icon" aria-label="Square help" onClick={click}>?</Button>
 <Button shape="icon" circular aria-label="Circular help" data-circle onClick={click}>?</Button>
 <Button shape="icon" circular size="compact" aria-label="Compact help" onClick={click}>?</Button>
 <Button shape="icon" circular disabled aria-label="Disabled help" onClick={click}>?</Button>
 <Button shape="icon" circular variant="primary" aria-label="Primary help" onClick={click}>?</Button>
 <Button shape="icon" circular variant="removalToggle" aria-pressed="true" aria-label="Remove help" onClick={click}>?</Button>
</section>,document.getElementById('root'));
window.controlGeometryReady=true;
