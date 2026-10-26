import { createSignal, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { rpc } from "../../transport";
import { Button } from "../ui/Button";
import type { AutomationRun } from "./contract";

interface CleanupPreview {
	automation_id: string;
	run_id: string;
	workspace: string | null;
	can_remove: boolean;
	reason: string | null;
}

/** The addressed backend owns provenance and every removal decision. */
export function WorktreeCleanup(props: { run: AutomationRun; connectionId?: string }) {
	const [preview, setPreview] = createSignal<CleanupPreview>();
	const [busy, setBusy] = createSignal(false);
	const [message, setMessage] = createSignal("");
	const [removed, setRemoved] = createSignal(false);
	async function act(remove: boolean) {
		if (busy()) return;
		setBusy(true);
		setMessage("");
		try {
			const input = { action: remove ? "cleanup_worktree" : "worktree_preview", id: props.run.id };
			const reply = props.connectionId
				? await rpc<CleanupPreview>("automation_action", { input }, props.connectionId)
				: await invoke<CleanupPreview>("automation_action", { input });
			if (remove) {
				setRemoved(true);
				setPreview(undefined);
				setMessage(t("automations.worktreeRemoved", "Worktree removed. Saved run history is retained."));
			} else setPreview(reply);
		} catch (error) {
			setPreview(undefined);
			setMessage(error instanceof Error ? error.message : String(error));
		} finally {
			setBusy(false);
		}
	}
	return (
		<Show when={props.run.workspace_id && !removed()} fallback={<p role="status">{message()}</p>}>
			<Button type="button" disabled={busy()} onClick={() => void act(false)}>
				{t("automations.inspectCleanup", "Inspect worktree cleanup")}
			</Button>
			<Show when={preview()}>
				{(value) => (
					<div>
						<p>{value().workspace}</p>
						<Show when={value().reason}>
							<p role="status">{value().reason}</p>
						</Show>
						<Button type="button" disabled={busy() || !value().can_remove} onClick={() => void act(true)}>
							{t("automations.removeWorktree", "Remove safe worktree and branch")}
						</Button>
					</div>
				)}
			</Show>
			<Show when={message()}>
				<p role="status">{message()}</p>
			</Show>
		</Show>
	);
}
