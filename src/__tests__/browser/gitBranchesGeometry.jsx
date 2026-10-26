import { render } from 'solid-js/web';
import { BranchesTab } from '../../components/GitPanel/BranchesTab';
import '../../global.css';

render(() => <section id="geometry" style="width:min(700px,100%);padding:16px;background:var(--bg-secondary)"><BranchesTab repoPath={null}/></section>, document.getElementById('root'));
// Measure the actual toolbar, not a duplicate collection of primitives.
const toolbar = document.querySelector('[title="New branch"]').parentElement;
toolbar.dataset.row = 'default';
const toggle = document.querySelector('[title="Disable prefix grouping"]');
toggle.click();
if (toggle.getAttribute('aria-pressed') !== 'false') throw new Error('Grouping toggle lost its owner state');
document.querySelector('[title="New branch"]').click();
requestAnimationFrame(() => {
  const create = [...document.querySelectorAll('button')].find(button => button.textContent.trim() === 'Create');
  if (!create) throw new Error('Create branch action did not open its form');
  const expected = innerWidth <= 600 ? 44 : 22;
  for (const control of create.parentElement.querySelectorAll('[data-form-control]')) {
    if (control.getBoundingClientRect().height !== expected) throw new Error('Compact create control geometry drifted');
  }
  window.controlGeometryReady = true;
});
