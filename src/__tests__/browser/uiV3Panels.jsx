import { render } from 'solid-js/web';
import { IssueDetailContent } from '../../components/IssueDetailPopover/IssueDetailContent';
import { PrDetailContent } from '../../components/PrDetailPopover/PrDetailContent';
import { PanelToolbar, ToolbarIcons } from '../../components/shared/PanelToolbar';
import { githubStore } from '../../stores/github';
import { promptLibraryStore } from '../../stores/promptLibrary';
import '../../global.css';
// Visual comparison against docs/frontend/ui-v3/issue-panel-reference.html; no remote GitHub operation runs.
let callback = 0;
window.__TAURI_INTERNALS__ = {
  transformCallback: () => ++callback,
  invoke: async (command, args) => {
    if (command === 'get_github_viewer_login') return 'bob';
    if (command === 'visit_work_item') return args?.itemUrl?.endsWith('/212') ? { count: 2, sinceMs: Date.now() - 3 * 36e5 } : { count: 0, sinceMs: null };
    if (command === 'workflow_neutral_action' && args?.action?.action === 'list_drafts')
      return { type: 'drafts', value: [{ id: 'triage', name: 'Triage and fix', latest_published_revision: 3 }, { id: 'release', name: 'Release notes', latest_published_revision: 1 }] };
    if (command === 'plugin:event|listen') return ++callback;
    if (command === 'get_pr_action_state') return {};
    if (command === 'get_pr_check_details') return [];
    return null;
  },
};
const base = { state: 'OPEN', url: '', head_ref_oid: 'a'.repeat(40), base_ref_name: 'main', author: 'alice', commits: 3, additions: 120, deletions: 14, labels: [{ name: 'bug', color: 'd73a4a' }], mergeable: 'MERGEABLE', conflict_state: 'clear', merge_state_status: 'CLEAN', merge_state_label: null, review_state_label: null, created_at: new Date(Date.now() - 3 * 864e5).toISOString(), updated_at: new Date(Date.now() - 2 * 36e5).toISOString(), checks: { passed: 9, failed: 0, pending: 0, total: 9 }, check_details: [], is_draft: false, review_decision: 'REVIEW_REQUIRED', viewer_did_approve: false, merge_commit_allowed: true, squash_merge_allowed: true, rebase_merge_allowed: true, unresolved_threads: 0, unresolved_threads_truncated: false };
githubStore.updateRepoData('/fixture', [
  { ...base, number: 41, branch: 'fix/review', title: 'Sidebar PR panel hides the Approve button below the check list' },
  { ...base, number: 42, branch: 'fix/red', title: 'Tasks search burns the GitHub quota', author: 'bob', checks: { passed: 7, failed: 2, pending: 0, total: 9 } },
]);
try { githubStore.startPolling(); } catch (e) { console.warn('viewer login unavailable', e); }
const issue = (patch) => ({ number: 212, title: 'Sidebar PR panel hides the Approve button below the check list', state: 'OPEN', url: '', author: 'jklap', labels: [{ name: 'bug', color: 'd73a4a' }, { name: 'ui', color: '0e8a16' }], assignees: [], milestone: null, comments_count: 4, created_at: new Date(Date.now() - 3 * 864e5).toISOString(), updated_at: null, ...patch });
const toolbar = (n) => <PanelToolbar menuLabel="More actions" actions={[{ id: 'gh', label: 'GitHub', icon: <ToolbarIcons.external />, onClick: () => {} }, { id: 'copy', label: `#${n}`, icon: <ToolbarIcons.copy />, onClick: () => {} }, { id: 'tasks', label: 'Tasks', icon: <ToolbarIcons.tasks />, onClick: () => {} }]} menu={[{ id: 'close', label: 'Close issue', divider: true, onSelect: () => {} }]} />;
const card = (title, body) => <div style="width:340px;border:1px solid var(--border);border-radius:6px;background:var(--bg-secondary)"><div style="padding:6px 12px;font-size:12px;color:var(--fg-primary);font-weight:600">{title}</div><div data-compact>{body}</div></div>;
await promptLibraryStore.hydrate();
render(() => <section id="geometry" style="display:flex;flex-wrap:wrap;gap:16px;padding:16px;background:var(--bg-primary);align-items:flex-start">
  {card('#212 Work on it', <IssueDetailContent issue={issue({})} repoPath="/fixture" onAutofix={() => {}}>{toolbar(212)}</IssueDetailContent>)}
  {card('#212 New comments', <IssueDetailContent issue={issue({ url: 'https://github.com/acme/tuic/issues/212' })} repoPath="/fixture" onAutofix={() => {}}>{toolbar(212)}</IssueDetailContent>)}
  {card('#209 Branch ready', <IssueDetailContent issue={issue({ number: 209, comments_count: 1 })} repoPath="/fixture" autofixBranchReady onCreatePr={() => {}} onOpenBranch={() => {}}>{toolbar(209)}</IssueDetailContent>)}
  {card('#41 Review (viewer bob)', <PrDetailContent repoPath="/fixture" branch="fix/review" onApprove={() => {}} />)}
  {card('#42 Own PR, red CI', <PrDetailContent repoPath="/fixture" branch="fix/red" />)}
</section>, document.getElementById('root'));
