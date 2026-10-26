import { render } from 'solid-js/web';
import { Toolbar } from '../../components/Toolbar/Toolbar';
import '../../global.css';

render(() => <section id="geometry" data-control-size="compact" style="background:var(--bg-primary)"><Toolbar repoPath="/geometry"/></section>,document.getElementById('root'));
window.controlGeometryReady=true;
