import { render } from 'solid-js/web';
import { Terminal } from '../../components/Terminal/Terminal';
import { terminalsStore } from '../../stores/terminals';
import '../../global.css';

// Render the real terminal boundary without a PTY. Inactive suspended tabs do
// not spawn or resume a process; layout and search use their production paths.
const cases = [1000, 400].flatMap(width => [true, false].map(context => {
  const id = terminalsStore.add({ fontSize: 14, name: 'Search geometry', cwd: '/geometry', agentType: 'claude', agentSessionId: 'geometry' });
  terminalsStore.update(id, { suspended: true, agentIntent: context ? 'Verify search controls stay reachable' : null });
  return { width, context, id };
}));
const ids = cases.map(sample => sample.id);
render(() => <main style={{ padding: '16px', display: 'grid', gap: '24px' }}>
  {cases.map(({ width, context, id }) => <section data-case={`${width}-${context}`} style={{ width: `min(${width}px, calc(100vw - 32px))`, height: '220px', border: '1px solid var(--border)' }}>
    <Terminal id={id} />
  </section>)}
</main>, document.getElementById('root'));
await new Promise(resolve => requestAnimationFrame(resolve));
for (const id of ids) terminalsStore.get(id).ref.openSearch();

window.terminalSearchGeometry = () => {
  const box = element => {
    if (!element) throw new Error('Missing terminal control');
    const r = element.getBoundingClientRect();
    return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, width: r.width, height: r.height };
  };
  const hit = element => {
    element.scrollIntoView({ block: 'center' });
    const r = element.getBoundingClientRect();
    return element.contains(document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2));
  };
  return [...document.querySelectorAll('[data-case]')].map(pane => {
    const toggle = pane.querySelector('[data-terminal-view-toggle]');
    const input = pane.querySelector('input[placeholder="Find…"]');
    const search = input.closest('[class*="overlay"]');
    const controls = [...search.querySelectorAll('button')];
    return { name: pane.dataset.case, pane: box(pane), toggle: box(toggle), search: box(search),
      controls: [...controls, ...toggle.querySelectorAll('button')].map(button => ({ title: button.title || button.textContent, box: box(button), hit: hit(button) })) };
  });
};
window.terminalSearchCloseAndReopen = () => {
  const pane = document.querySelector('[data-case]');
  pane.querySelector('button[title="Close (Escape)"]').click();
  const closed = !pane.querySelector('input[placeholder="Find…"]');
  terminalsStore.get(ids[0]).ref.openSearch();
  return closed && !!pane.querySelector('input[placeholder="Find…"]');
};
