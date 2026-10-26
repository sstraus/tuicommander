import { type Component, createSignal, onCleanup, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import type { ProviderRecovery } from "../../stores/terminals";
import { rpc } from "../../transport";
import { Button } from "../ui";
import s from "./ProviderRecoveryStatus.module.css";

export function recoveryLabel(recovery: ProviderRecovery, now: number): string {
	const next = recovery.next_attempt_ms;
	return `Waiting for provider recovery · ${recovery.phase} · attempt ${recovery.attempt}${next === null ? "" : ` · next ${new Date(next).toLocaleTimeString()} (${Math.max(0, Math.ceil((next - now) / 1000))}s)`}`;
}

export const ProviderRecoveryStatus: Component<{ recovery: ProviderRecovery; sessionId: string }> = (props) => {
	const [now, setNow] = createSignal(Date.now());
	const [sending, setSending] = createSignal(false);
	const [failure, setFailure] = createSignal<string>();
	const timer = setInterval(() => setNow(Date.now()), 1000);
	onCleanup(() => clearInterval(timer));
	const retry = async () => {
		if (sending()) return;
		setSending(true);
		setFailure(undefined);
		const sessionId = props.sessionId;
		try {
			await rpc("force_retry_provider_recovery_command", { sessionId });
		} catch (error) {
			setFailure(String(error));
			appLogger.warn("terminal", "Force retry failed", error);
		} finally {
			setSending(false);
		}
	};
	return (
		<span role="status" class={s.recovery}>
			<span class={s.label} title={recoveryLabel(props.recovery, now())}>
				{recoveryLabel(props.recovery, now())}
			</span>
			<Button size="compact" type="button" disabled={sending()} onClick={retry}>
				Force retry
			</Button>
			<Show when={failure()}>{(message) => <span role="alert">{message()}</span>}</Show>
		</span>
	);
};
