import { createEffect, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { SettingsSearchBox } from '../../components/SettingsPanel/SettingsSearch';
import { RemoteAccessPanel } from '../../components/SettingsPanel/tabs/services/RemoteAccessPanel';
import { ColorSwatchPicker } from '../../components/shared/ColorSwatchPicker';
import { DEFAULT_COLOR_PRESETS } from '../../components/shared/colorPresets';
import '../../global.css';

const [query, setQuery] = createSignal('Remote access');
const [color, setColor] = createSignal('#4A9EFF');
render(() => <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)">
 <SettingsSearchBox value={query()} onInput={setQuery}/>
 <ColorSwatchPicker color={color()} presets={DEFAULT_COLOR_PRESETS} onChange={setColor}/>
 <RemoteAccessPanel/>
</section>, document.getElementById('root'));
createEffect(() => {
 requestAnimationFrame(function ready() {
  if(document.querySelector('button[title="Recheck Tailscale status"]')) window.controlGeometryReady=true;
  else requestAnimationFrame(ready);
 });
});
