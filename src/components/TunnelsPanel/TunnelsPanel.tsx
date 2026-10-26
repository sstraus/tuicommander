import { type Component, createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { tunnelPanelStore } from "../../stores/tunnelPanel";
import type { TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";

interface AuditEntry {
	tunnel_id: string;
	timestamp: string;
	kind: string;
	message: string | null;
}

import { Button } from "../ui/Button";
import { TunnelEditorModal } from "./TunnelEditorModal";
import { TunnelStatusBadge } from "./TunnelStatusBadge";
import s from "./TunnelsPanel.module.css";

export const TunnelsPanel: Component = () => {
	const isOpen = () => tunnelPanelStore.state.isOpen;
	const [editorProfile, setEditorProfile] = createSignal<TunnelProfile | undefined>(undefined);
	const [editorVisible, setEditorVisible] = createSignal(false);
	const [expandedId, setExpandedId] = createSignal<string | null>(null);
	const [auditEntries, setAuditEntries] = createSignal<AuditEntry[]>([]);

	const toggleExpand = async (id: string) => {
		if (expandedId() === id) {
			setExpandedId(null);
			setAuditEntries([]);
			return;
		}
		setExpandedId(id);
		try {
			const entries = await invoke<AuditEntry[]>("get_tunnel_audit", { id, limit: 20 });
			if (expandedId() !== id) return;
			setAuditEntries(entries ?? []);
		} catch {
			if (expandedId() !== id) return;
			setAuditEntries([]);
		}
	};

	onMount(() => {
		tunnelsStore.hydrate();
	});

	// Escape to close
	createEffect(() => {
		if (!isOpen()) return;

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				e.preventDefault();
				e.stopPropagation();
				if (editorVisible()) {
					setEditorVisible(false);
				} else {
					tunnelPanelStore.close();
				}
			}
		};

		document.addEventListener("keydown", handleKeydown, true);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
	});

	const openEditor = (profile?: TunnelProfile) => {
		setEditorProfile(profile);
		setEditorVisible(true);
	};

	const handleDelete = async (id: string) => {
		try {
			await tunnelsStore.deleteProfile(id);
		} catch (err) {
			appLogger.error("store", "TunnelsPanel delete failed", err);
		}
	};

	const handleToggleTunnel = async (id: string) => {
		const active = tunnelsStore.getTunnelStatus(id);
		try {
			if (active && active.type !== "stopped" && active.type !== "error") {
				await tunnelsStore.stopTunnel(id);
			} else {
				await tunnelsStore.startTunnel(id);
			}
		} catch (err) {
			appLogger.error("store", "TunnelsPanel toggle tunnel failed", err);
		}
	};

	const isRunning = (id: string): boolean => {
		const status = tunnelsStore.getTunnelStatus(id);
		return !!status && status.type !== "stopped" && status.type !== "error";
	};

	return (
		<Show when={isOpen()}>
			<div class={s.overlay} onClick={(e) => e.target === e.currentTarget && tunnelPanelStore.close()}>
				<div class={s.dashboard}>
					{/* Header */}
					<div class={s.header}>
						<h3>SSH Tunnels</h3>
						<div class={s.headerActions}>
							<Button class={s.newBtn} onClick={() => openEditor()}>
								+ New Tunnel
							</Button>
							<Button variant="ghost" class={s.closeBtn} onClick={() => tunnelPanelStore.close()} title="Close">
								<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
									<path d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z" />
								</svg>
							</Button>
						</div>
					</div>

					{/* Profile list */}
					<div class={s.list}>
						<Show
							when={tunnelsStore.getProfiles().length > 0}
							fallback={<div class={s.empty}>No tunnel profiles yet. Click "+ New Tunnel" to create one.</div>}
						>
							<For each={tunnelsStore.getProfiles()}>
								{(profile) => (
									<>
										<div class={s.row}>
											<div class={s.rowInfo}>
												<span class={s.rowName}>{profile.name}</span>
												<span class={s.rowMeta}>
													{profile.user}@{profile.host}:{profile.port}
												</span>
												<TunnelStatusBadge status={tunnelsStore.getTunnelStatus(profile.id)} />
											</div>
											<div class={s.rowActions}>
												<Button
													variant="ghost"
													class={s.actionBtn}
													onClick={() => handleToggleTunnel(profile.id)}
													title={isRunning(profile.id) ? "Stop" : "Start"}
												>
													{isRunning(profile.id) ? "Stop" : "Start"}
												</Button>
												<Button variant="ghost" class={s.actionBtn} onClick={() => openEditor(profile)} title="Edit">
													Edit
												</Button>
												<Button
													variant="ghost"
													class={s.actionBtn}
													onClick={() => toggleExpand(profile.id)}
													title="Audit log"
												>
													{expandedId() === profile.id ? "Hide" : "Log"}
												</Button>
												<Button
													variant="ghost"
													class={s.actionBtn}
													onClick={() => handleDelete(profile.id)}
													title="Delete"
												>
													Del
												</Button>
											</div>
										</div>
										<Show when={expandedId() === profile.id}>
											<div class={s.auditTimeline}>
												<Show
													when={auditEntries().length > 0}
													fallback={<span class={s.auditEmpty}>No audit entries</span>}
												>
													<For each={auditEntries()}>
														{(entry) => (
															<div class={s.auditEntry}>
																<span class={s.auditTime}>{new Date(entry.timestamp).toLocaleTimeString()}</span>
																<span class={s.auditKind}>{entry.kind}</span>
																<Show when={entry.message}>
																	<span class={s.auditMsg}>{entry.message}</span>
																</Show>
															</div>
														)}
													</For>
												</Show>
											</div>
										</Show>
									</>
								)}
							</For>
						</Show>
					</div>
				</div>
			</div>

			{/* Editor modal */}
			<Show when={editorVisible()}>
				<TunnelEditorModal profile={editorProfile()} onClose={() => setEditorVisible(false)} />
			</Show>
		</Show>
	);
};

export default TunnelsPanel;
