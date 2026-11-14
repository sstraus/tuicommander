import { createEffect, createSignal, onCleanup } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { getSessionConnection } from "../../transportRuntime";
import { writeClipboard } from "../../utils/clipboard";
import type { ContextMenuItem } from "./ContextMenu";

export function createSessionCopyItems(terminalId: () => string | null, visible: () => boolean) {
	type SessionIdentity = {
		session_id: string;
		child_pid?: number | null;
		alias?: string | null;
		connection_id?: string;
	};
	const [identity, setIdentity] = createSignal<SessionIdentity | null>(null);
	const menuTerminal = () => terminalsStore.get(terminalId() ?? "");
	const liveSessionId = () => {
		const term = menuTerminal();
		return term && term.shellState !== "exited" && !term.suspended ? term.sessionId : null;
	};
	const readIdentity = async (sessionId: string) => {
		const connectionId = getSessionConnection(sessionId);
		const rows = await invoke<SessionIdentity[]>("list_active_sessions");
		return rows.find((row) => row.session_id === sessionId && row.connection_id === connectionId) ?? null;
	};
	createEffect(() => {
		setIdentity(null);
		if (!visible()) return;
		const sessionId = liveSessionId();
		if (!sessionId) return;
		let cancelled = false;
		onCleanup(() => {
			cancelled = true;
		});
		void readIdentity(sessionId)
			.then((row) => {
				if (!cancelled) setIdentity(row);
			})
			.catch((error) => appLogger.warn("app", "Could not read session identifiers", error));
	});
	const copyId = async (kind: "session" | "process" | "alias") => {
		const sessionId = menuTerminal()?.sessionId;
		if (!sessionId) return;
		try {
			const row = kind !== "session" && liveSessionId() === sessionId ? await readIdentity(sessionId) : null;
			const value = kind === "session" ? sessionId : kind === "process" ? row?.child_pid : row?.alias;
			if (value == null || value === "")
				throw new Error(t("sidebar.identifierUnavailable", "Session identifier unavailable"));
			await writeClipboard(String(value));
			toastsStore.add(
				kind === "process"
					? t("sidebar.processIdCopied", "Process ID copied")
					: kind === "alias"
						? t("sidebar.aliasCopied", "Alias copied")
						: t("sidebar.sessionIdCopied", "Session ID copied"),
				String(value),
				"info",
				false,
				undefined,
				undefined,
				undefined,
				undefined,
				false,
			);
		} catch (error) {
			toastsStore.add(
				kind === "process"
					? t("sidebar.copyProcessIdFailed", "Could not copy process ID")
					: kind === "alias"
						? t("sidebar.copyAliasFailed", "Could not copy alias")
						: t("sidebar.copySessionIdFailed", "Could not copy session ID"),
				String(error),
				"error",
				false,
				undefined,
				undefined,
				undefined,
				undefined,
				false,
			);
		}
	};
	const sessionMenuItems = (): ContextMenuItem[] => [
		{
			label: t("sidebar.copySessionId", "Copy session ID"),
			title: menuTerminal()?.sessionId ?? undefined,
			disabled: !menuTerminal()?.sessionId,
			action: () => {
				void copyId("session");
			},
		},
		...(liveSessionId() && identity()?.child_pid != null
			? [
					{
						label: t("sidebar.copyProcessId", "Copy process ID"),
						title: String(identity()!.child_pid),
						action: () => {
							void copyId("process");
						},
					},
				]
			: []),
		...(liveSessionId() && identity()?.alias
			? [
					{
						label: t("sidebar.copyAlias", "Copy alias"),
						title: identity()!.alias!,
						action: () => {
							void copyId("alias");
						},
					},
				]
			: []),
	];
	return sessionMenuItems;
}
