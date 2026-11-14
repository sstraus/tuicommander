import { type Component, createSignal, Show } from "solid-js";
import { useRepository, type WorktreeDiagnostic } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { repositoriesStore } from "../../stores/repositories";
import { Button } from "../ui/Button";
import s from "./Sidebar.module.css";

const baseName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/** One compact row per unavailable worktree registration; detail lives in the tooltip. */
export const WorktreeDiagnosticRow: Component<{ repoPath: string; diagnostic: WorktreeDiagnostic }> = (props) => {
	const repo = useRepository();
	const [busy, setBusy] = createSignal(false);
	const [confirmPrune, setConfirmPrune] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const fixable = () => props.diagnostic.state === "missing";
	const label = () =>
		props.diagnostic.state === "duplicate_branch"
			? t("sidebar.diagnosticDuplicate", "duplicate branch")
			: props.diagnostic.checkout_present
				? t("sidebar.diagnosticBroken", "link broken")
				: t("sidebar.diagnosticGone", "folder missing");

	async function run(action: "repair" | "prune") {
		setBusy(true);
		setError(null);
		try {
			await repo.resolveWorktreeDiagnostic(props.repoPath, props.diagnostic.path, action);
			repositoriesStore.bumpGitRevision(props.repoPath);
		} catch (e) {
			setError(String(e).replace(/^Error:\s*/, ""));
		} finally {
			setBusy(false);
			setConfirmPrune(false);
		}
	}

	return (
		<div data-testid="worktree-diagnostic">
			<div class={s.worktreeDiagnostic} data-tooltip={`${props.diagnostic.path}\n${props.diagnostic.guidance}`}>
				<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
					<path d="M8 1.5 15 14H1L8 1.5Zm-.75 5v3.5h1.5V6.5h-1.5Zm0 4.5v1.5h1.5V11h-1.5Z" />
				</svg>
				<span class={s.diagnosticName}>{baseName(props.diagnostic.path)}</span>
				<span class={s.detachedState}>{label()}</span>
				<Show when={fixable()}>
					<Show
						when={props.diagnostic.checkout_present}
						fallback={
							<Button
								size="compact"
								variant={confirmPrune() ? "danger" : "ghost"}
								disabled={busy()}
								data-testid="worktree-diagnostic-prune"
								data-tooltip={t(
									"sidebar.diagnosticPruneHint",
									"Drop the registration of the deleted folder; no files are deleted",
								)}
								onClick={() => (confirmPrune() ? run("prune") : setConfirmPrune(true))}
								onBlur={() => setConfirmPrune(false)}
							>
								{confirmPrune()
									? t("sidebar.diagnosticPruneConfirm", "Confirm?")
									: t("sidebar.diagnosticPrune", "Prune")}
							</Button>
						}
					>
						<Button
							size="compact"
							variant="ghost"
							disabled={busy()}
							data-testid="worktree-diagnostic-repair"
							data-tooltip={t("sidebar.diagnosticRepairHint", "Run git worktree repair on this checkout")}
							onClick={() => run("repair")}
						>
							{t("sidebar.diagnosticRepair", "Repair")}
						</Button>
					</Show>
				</Show>
			</div>
			<Show when={error()}>
				<div class={s.diagnosticError} data-testid="worktree-diagnostic-error" data-tooltip={error()}>
					{error()}
				</div>
			</Show>
		</div>
	);
};
