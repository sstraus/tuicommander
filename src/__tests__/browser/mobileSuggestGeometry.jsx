import { render } from 'solid-js/web';
import { SuggestChips } from '../../mobile/components/SuggestChips';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
// Catches compressed suggestion targets and a long list widening its host.
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <SuggestChips sessionId="geometry-only" items={['1) Run tests','2. Review diff','Inspect a very long suggestion that must stay horizontally scrollable without shrinking its target']}/>
</section>, document.getElementById('root'));
const buttons=[...document.querySelectorAll('#geometry button')];
if(buttons.length!==3||buttons[0].textContent!=='Run tests'||buttons[1].textContent!=='Review diff')throw new Error('Suggestion labels changed');
const row=buttons[0].parentElement;
if(getComputedStyle(row).overflowX!=='auto')throw new Error('Suggestions must scroll horizontally');
if(document.documentElement.scrollWidth>innerWidth)throw new Error('Suggestions widened the page');
window.controlGeometryReady=true;
