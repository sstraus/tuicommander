import { render } from 'solid-js/web';
import { SmartButtonStrip } from '../../components/SmartButtonStrip/SmartButtonStrip';
import { promptLibraryStore } from '../../stores/promptLibrary';
import s from '../../components/SmartButtonStrip/SmartButtonStrip.module.css';
import '../../global.css';

const make = (name, placement) => promptLibraryStore.createPrompt({ name, content: 'Geometry only', category: 'custom', isFavorite: false, tags: ['smart','geometry-fixture'], placement: [placement], executionMode: 'shell' });
make('Review changes', 'file-context');
make('Explain changes', 'file-context');
make('A single long prompt name that must remain inside the available strip', 'terminal-context');
render(() => <section id="geometry" style="padding:16px;width:360px;height:320px;display:flex;flex-direction:column;align-items:flex-end;gap:16px;background:var(--bg-secondary)">
  <SmartButtonStrip placement="file-context" repoPath="/geometry" extraFilter={p => p.tags?.includes('geometry-fixture')} />
  <SmartButtonStrip placement="terminal-context" repoPath="/geometry" extraFilter={p => p.tags?.includes('geometry-fixture')} />
</section>, document.getElementById('root'));
document.querySelector('[title="More actions"]').click();
document.querySelectorAll('[role="group"]').forEach(group => { group.dataset.joinedCase = 'compact'; if(group.children.length > 1) group.dataset.row = 'compact'; });
document.querySelector(`.${s.menu}`).dataset.row = 'default';
window.controlGeometryReady = true;
