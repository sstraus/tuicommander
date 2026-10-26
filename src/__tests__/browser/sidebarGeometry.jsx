import { render } from 'solid-js/web';
import { Sidebar } from '../../components/Sidebar/Sidebar';
import { BranchItem } from '../../components/Sidebar/RepoSection';
import { Button } from '../../components/ui/Button';
import s from '../../components/Sidebar/Sidebar.module.css';
import '../../global.css';

const noop = () => {};
const branch = { workspaceId: 'geometry', branchName: 'fix/long-sidebar-control-label', worktreePath: '/geometry/worktree', terminals: [], additions: 0, deletions: 0, isMain: false, tabsCollapsed: false };
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary);display:flex;flex-direction:column;gap:16px;width:360px">
  <div data-row="compact"><Button size="compact">Reference</Button><Button size="compact" variant="ghost">Ghost reference</Button></div>
  <Sidebar onBranchSelect={noop} onAddTerminal={noop} onRemoveBranch={noop} onRenameBranch={noop} onAddWorktree={noop} onAddRepo={noop} onRepoSettings={noop} onRemoveRepo={noop} onOpenSettings={noop} onOpenHelp={noop} />
  <div class={s.sidebar} data-density="rich">
    <BranchItem branch={branch} repoPath="/geometry" isActive={false} canRemove={true} openSwipeRow={JSON.stringify(['/geometry', 'geometry'])} onSelect={noop} onAddTerminal={noop} onRemove={noop} onRename={noop} onShowPrDetail={noop} />
  </div>
</section>, document.getElementById('root'));
document.querySelectorAll(`.${s.footerIcons}`).forEach(row => { row.dataset.row = 'compact'; });
const tray = document.querySelector(`.${s.branchSwipeActions}`);
tray.dataset.stretchCase = String(tray.getBoundingClientRect().height);
window.controlGeometryReady = true;
