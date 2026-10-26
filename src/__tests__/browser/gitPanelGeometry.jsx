import { render } from 'solid-js/web';
import { GitPanel } from '../../components/GitPanel/GitPanel';
import { Select } from '../../components/ui/Select';
import '../../global.css';
render(() => <section id="geometry" style="width:min(700px,100%);padding:16px;background:var(--bg-secondary)"><GitPanel mode="detached" visible={true} repoPath={null} onClose={()=>{}}/></section>,document.getElementById('root'));
const tabs = document.querySelector('[aria-label="Git views"]');
// Compare the real mounted tablist against a shared field in the same row.
const field = document.createElement('div');
tabs.parentElement.append(field);
render(() => <Select aria-label="Geometry reference"><option>Reference</option></Select>, field);
field.style.display = 'contents';
// Measure the tablist and reference without including the wrapper.
const row = document.createElement('div');
row.dataset.row = 'default';
row.style.display = 'flex';
row.style.gap = '8px';
tabs.parentElement.insertBefore(row,tabs);
row.append(tabs,field.firstElementChild);
const log = [...tabs.querySelectorAll('[role="tab"]')].find(tab=>tab.textContent==='Log');
log.click();
if(log.getAttribute('aria-selected')!=='true') throw new Error('Git view selection became inert');
window.controlGeometryReady=true;
