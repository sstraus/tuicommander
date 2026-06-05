import type { BranchDetail } from "../components/GitPanel/types";

/**
 * Value-equality for two branch-detail lists. Used by BranchesTab to skip
 * `setBranches` when a revision bump produced an identical list (e.g. staging a
 * file bumps the repo revision but the branch list is unchanged), so SolidJS
 * doesn't re-reconcile identical rows. All BranchDetail fields are primitives,
 * so a field-by-field compare is exact and order-insensitive to key layout.
 */
export function branchListsEqual(a: readonly BranchDetail[], b: readonly BranchDetail[]): boolean {
	if (a === b) return true;
	if (a.length !== b.length) return false;
	for (let i = 0; i < a.length; i++) {
		const x = a[i];
		const y = b[i];
		if (
			x.name !== y.name ||
			x.is_current !== y.is_current ||
			x.is_remote !== y.is_remote ||
			x.is_main !== y.is_main ||
			x.is_merged !== y.is_merged ||
			x.ahead !== y.ahead ||
			x.behind !== y.behind ||
			x.upstream !== y.upstream ||
			x.last_commit_date !== y.last_commit_date ||
			x.last_commit_message !== y.last_commit_message ||
			x.last_commit_author !== y.last_commit_author ||
			x.base_ahead !== y.base_ahead ||
			x.base_behind !== y.base_behind ||
			x.base_branch !== y.base_branch
		) {
			return false;
		}
	}
	return true;
}
