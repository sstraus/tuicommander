import { For, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { rpc } from "../../transport";
import { sendCommand } from "../../utils/sendCommand";
import type { AgentCommand } from "../config/agentCommands";
import { getAgentCommands } from "../config/agentCommands";
import { retryWrite } from "../utils/retryWrite";
import styles from "./CommandWidget.module.css";

interface CommandWidgetProps {
	sessionId: string;
	agentType: string | null | undefined;
	onDismiss: () => void;
}

export function CommandWidget(props: CommandWidgetProps) {
	const commandSet = () => getAgentCommands(props.agentType);

	async function send(text: string) {
		props.onDismiss();
		try {
			// Route through the canonical sendCommand helper: it handles the
			// agent-specific Ctrl-U prefix, the split Enter (Ink raw mode), and
			// bracketed-paste for multi-line input — and skips Ctrl-U on native
			// Windows shells where it would echo literally.
			await sendCommand(
				(data) => retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data })),
				text,
				props.agentType,
			);
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("network", `Command send failed after retries: ${msg}`);
		}
	}

	async function sendModel(model: string) {
		const cmd = commandSet().modelCommand;
		if (!cmd) return;
		await send(`${cmd} ${model}`);
	}

	async function sendPermissionToggle() {
		const seq = commandSet().permissionToggleSeq;
		if (!seq) return;
		props.onDismiss();
		try {
			await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: seq }));
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("network", `Permission toggle failed after retries: ${msg}`);
		}
	}

	const handleBackdropClick = (e: MouseEvent) => {
		if (e.target === e.currentTarget) {
			props.onDismiss();
		}
	};

	const hasCommands = () => commandSet().commands.length > 0;
	const hasModels = () => (commandSet().models?.length ?? 0) > 0;
	const hasPermissionToggle = () => !!commandSet().permissionToggleSeq;
	const hasAnything = () => hasCommands() || hasModels() || hasPermissionToggle();

	return (
		<Show when={hasAnything()}>
			<div class={styles.backdrop} onClick={handleBackdropClick}>
				<div class={styles.sheet}>
					<Show when={hasCommands()}>
						<div class={styles.sectionLabel}>Commands</div>
						<div class={styles.commandRow}>
							<For each={commandSet().commands}>
								{(cmd: AgentCommand) => (
									<button class={styles.chip} onClick={() => send(cmd.command)}>
										{cmd.label}
									</button>
								)}
							</For>
						</div>
					</Show>

					<Show when={hasModels()}>
						<div class={styles.sectionLabel}>Model</div>
						<div class={styles.commandRow}>
							<For each={commandSet().models}>
								{(model: string) => (
									<button class={styles.modelChip} onClick={() => sendModel(model)}>
										{model}
									</button>
								)}
							</For>
						</div>
					</Show>

					<Show when={hasPermissionToggle()}>
						<div class={styles.sectionLabel}>Controls</div>
						<div class={styles.commandRow}>
							<button class={styles.permissionChip} onClick={sendPermissionToggle}>
								Permission Toggle
							</button>
						</div>
					</Show>
				</div>
			</div>
		</Show>
	);
}
