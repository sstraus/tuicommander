import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { toastsStore } from "../stores/toasts";

/**
 * Owner control for literal terminal diagnostic reads (#2643-6b38).
 *
 * Terminal reads mask secret-shaped text by default, so a fake fixture such as
 * `TEST_TOKEN=not-a-secret` disappears while an agent debugs it. The owner can
 * grant one session literal reads from its tab. The grant lives in the backend
 * and is set only through desktop IPC: loopback HTTP and MCP are reachable by
 * models, so neither can grant it. Registered secrets stay masked.
 */
const [granted, setGranted] = createSignal<ReadonlySet<string>>(new Set());

type GrantStatus = { session_id: string; enabled: boolean };

export const literalReadsStore = {
	isGranted(sessionId: string): boolean {
		return granted().has(sessionId);
	},

	async toggle(sessionId: string): Promise<void> {
		const enable = !this.isGranted(sessionId);
		try {
			const next = await invoke<GrantStatus>("set_terminal_literal_reads", { sessionId, enabled: enable });
			const copy = new Set(granted());
			if (next?.enabled) copy.add(sessionId);
			else copy.delete(sessionId);
			setGranted(copy);
			toastsStore.add(
				enable ? "Literal reads allowed" : "Literal reads revoked",
				enable
					? "Agents that ask for literal output of this terminal now see secret-shaped text. Registered secrets stay masked."
					: "Terminal reads of this session are masked again.",
				enable ? "warn" : "info",
			);
		} catch (err) {
			appLogger.error("app", "[LiteralReads] toggle failed", err);
			toastsStore.add("Literal reads not changed", String(err), "error");
		}
	},
};
