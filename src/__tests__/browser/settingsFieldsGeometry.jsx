import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { SettingAccount, SettingButton, SettingList } from '../../components/SettingsPanel/SettingFields';
import '../../global.css';

// Actual field composites; account data is a safe layout-only fixture.
const [rows, setRows] = createSignal([{id:'one', values:{name:'Existing row'}}]);
render(() => <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)">
  <SettingButton label="Connection" value="Test connection" onClick={() => {}} />
  <SettingButton label="Unavailable connection" value="Unavailable action" disabled onClick={() => {}} />
  <SettingAccount label="Account" provider="fixture" value="one" onInput={() => {}} onAdd={async () => {}}
    listAccounts={async () => [{id:'one', provider:'fixture', label:'Fixture account'}]} />
  <SettingList label="Rows" rows={rows()} onChange={setRows} columns={[{id:'name',label:'Name'}]}
    onAdd={() => ({id:'new', values:{name:'New row'}})} />
</section>, document.getElementById('root'));
requestAnimationFrame(() => { window.controlGeometryReady = true; });
