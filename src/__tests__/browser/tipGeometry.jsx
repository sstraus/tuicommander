import { render } from 'solid-js/web';
import TipOfTheDay from '../../components/TipOfTheDay/TipOfTheDay';
import { Select } from '../../components/ui';
import '../../global.css';
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <TipOfTheDay/><Select aria-label="Reference status"><option>Ready</option></Select>
</section>,document.getElementById('root'));
window.controlGeometryReady=true;
