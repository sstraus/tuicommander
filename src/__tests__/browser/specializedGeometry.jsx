import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Switch, Slider, Select, ColorInput, ColorSwatch } from '../../components/ui';
import { SettingToggle, SettingSlider } from '../../components/SettingsPanel/SettingFields';
import { SettingRow } from '../../components/SettingsPanel/SettingsPrimitives';
import '../../global.css';

const [checked, setChecked] = createSignal(true);
const [value, setValue] = createSignal(5);
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
 <SettingRow label="Shared controls">
  {() => <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
   <Switch aria-label="Enabled" checked={checked()} onChange={e => setChecked(e.currentTarget.checked)} />
   <Slider aria-label="Level" min={0} max={10} value={value()} onInput={e => setValue(e.currentTarget.valueAsNumber)} />
   <Select aria-label="Mode"><option>Default</option></Select>
   <ColorInput aria-label="Color" value="#59a8dd" />
   <ColorSwatch aria-label="Blue preset" swatchColor="#59a8dd" selected />
   <ColorSwatch aria-label="Clear">×</ColorSwatch>
  </div>}
 </SettingRow>
 <SettingToggle row label="Settings switch" checked={checked()} onChange={setChecked}/>
 <SettingSlider row label="Settings slider" min={0} max={10} value={value()} onChange={setValue}/>
</section>, document.getElementById('root'));
window.controlGeometryReady = true;
