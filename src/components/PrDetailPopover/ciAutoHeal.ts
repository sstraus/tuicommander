import { t } from "../../i18n";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import type { PanelToolbarMenuItem } from "../shared/PanelToolbar";

/** The per-branch CI auto-heal switch as a `⋯` menu item (ui-v3: toggles live in the menu).
 *  The counters live on a workspace row, reached through the single branch->id seam
 *  (`useCiHeal` resolves the same way, so the switch and the healer address the same record). */
export function ciAutoHealMenuItem(repoPath: string, branch: string): PanelToolbarMenuItem {
	const workspaceId = repositoriesStore.workspaceIdOnBranch(repoPath, branch) ?? branch;
	const state = repositoriesStore.state.repositories[repoPath]?.workspaces[workspaceId]?.ciAutoHeal;
	const enabled = state?.enabled ?? false;
	const attempts = state?.attempts ?? 0;
	const status = state?.healing
		? ` · ${t("prDetail.healing", "Healing")} ${attempts}/3`
		: enabled && attempts > 0
			? ` · ${attempts}/3 ${t("prDetail.attempts", "attempts")}`
			: "";
	return {
		id: "auto-heal",
		label: `${t("prDetail.autoHealCi", "Auto-heal CI")}: ${enabled ? t("prDetail.on", "on") : t("prDetail.off", "off")}${status}`,
		divider: true,
		onSelect: () => {
			const turningOn = !enabled;
			repositoriesStore.setCiAutoHeal(repoPath, workspaceId, {
				enabled: turningOn,
				attempts: turningOn ? attempts : 0,
			});
			// Enabling while the PR is already blocked: the green→red / →conflict transition event
			// has already passed, so start a heal attempt now. CI failures win over conflicts.
			if (!turningOn) return;
			const pr = githubStore.getBranchPrData(repoPath, branch);
			if (!pr) return;
			if ((githubStore.getCheckSummary(repoPath, branch)?.failed ?? 0) > 0) {
				githubStore.triggerCiHeal(repoPath, branch, pr.number);
			} else if (pr.mergeable === "CONFLICTING") {
				githubStore.triggerConflictHeal(repoPath, branch, pr.number);
			}
		},
	};
}
