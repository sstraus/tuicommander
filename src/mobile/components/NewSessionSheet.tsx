import { createSignal, For, onMount, Show } from "solid-js";
import { AGENTS, type AgentConfig, type AgentType } from "../../agents";
import { Button } from "../../components/ui/Button";
import { Input } from "../../components/ui/Input";
import { Select } from "../../components/ui/Select";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import styles from "./NewSessionSheet.module.css";

interface NewSessionSheetProps {
	repos: string[];
	onDismiss: () => void;
	onCreated?: (sessionId: string) => void;
}

function repoName(path: string): string {
	const parts = path.split("/");
	return parts.at(-1)! || path;
}

export function NewSessionSheet(props: NewSessionSheetProps) {
	const [agents, setAgents] = createSignal<(AgentConfig & { path: string })[]>([]);
	const [agent, setAgent] = createSignal<AgentType>();
	const [detecting, setDetecting] = createSignal(true);
	const [detectionError, setDetectionError] = createSignal<string>();
	onMount(async () => {
		try {
			const candidates = Object.values(AGENTS).filter((entry) => entry.type !== "git" && entry.type !== "api");
			const detections = await rpc<Record<string, { path: string | null }>>("detect_all_agent_binaries", {
				binaries: candidates.map((entry) => entry.binary),
			});
			const installed = candidates.flatMap((entry) => {
				const path = detections[entry.binary]?.path;
				return path ? [{ ...entry, path }] : [];
			});
			setAgents(installed);
			setAgent(installed[0]?.type);
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			setDetectionError(`Could not detect installed agents: ${message}`);
		} finally {
			setDetecting(false);
		}
	});
	const [query, setQuery] = createSignal("");
	const [creating, setCreating] = createSignal(false);
	const visibleRepos = () => props.repos.filter((repo) => repo.toLowerCase().includes(query().trim().toLowerCase()));

	async function createSession(cwd: string) {
		const selected = agents().find((entry) => entry.type === agent());
		if (creating() || detecting() || !selected) return;
		setCreating(true);
		try {
			const sessionId = await rpc<string>("spawn_agent", {
				pty_config: { cwd, rows: 24, cols: 80 },
				agent_config: {
					cwd,
					agent_type: selected.type,
					binary_path: selected.path,
					prompt: "",
					print_mode: false,
					args: [],
				},
			});
			props.onCreated?.(sessionId);
			props.onDismiss();
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.warn("network", `Failed to create session: ${msg}`);
			toastsStore.add("Session failed", `Could not create session: ${msg}`, "error", true);
		} finally {
			setCreating(false);
		}
	}

	const handleBackdropClick = (e: MouseEvent) => {
		if (e.target === e.currentTarget) props.onDismiss();
	};

	return (
		<div class={styles.backdrop} onClick={handleBackdropClick}>
			<div class={styles.sheet} role="dialog" aria-label="New session">
				<div class={styles.header}>
					<div class={styles.title}>New Session</div>
					<Button shape="icon" variant="ghost" aria-label="Close new session" onClick={props.onDismiss}>
						<svg
							width="20"
							height="20"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							aria-hidden="true"
						>
							<path d="M18 6 6 18M6 6l12 12" />
						</svg>
					</Button>
				</div>
				<label class={styles.fieldLabel} for="new-session-agent">
					Agent
				</label>
				<Select
					id="new-session-agent"
					class={styles.field}
					value={agent() ?? ""}
					disabled={detecting() || !agent() || creating()}
					onChange={(event) => setAgent(event.currentTarget.value as AgentType)}
				>
					<For each={agents()}>{(choice) => <option value={choice.type}>{choice.name}</option>}</For>
				</Select>
				<Show when={detecting()}>
					<div class={styles.empty} role="status">
						Detecting installed agents…
					</div>
				</Show>
				<Show when={detectionError()}>
					<div class={styles.empty} role="alert">
						{detectionError()}
					</div>
				</Show>
				<Show when={!detecting() && !detectionError() && agents().length === 0}>
					<div class={styles.empty} role="status">
						No installed agents found
					</div>
				</Show>
				<label class={styles.fieldLabel} for="new-session-repo">
					Repository
				</label>
				<Input
					id="new-session-repo"
					class={styles.field}
					type="search"
					aria-label="Search repositories"
					placeholder="Search repositories"
					value={query()}
					onInput={(event) => setQuery(event.currentTarget.value)}
				/>
				<Show when={props.repos.length > 0} fallback={<div class={styles.empty}>No repositories configured</div>}>
					<Show when={visibleRepos().length > 0} fallback={<div class={styles.empty}>No matching repositories</div>}>
						<For each={visibleRepos()}>
							{(repo) => (
								<button
									type="button"
									data-control-layout="row"
									class={styles.repoItem}
									disabled={creating() || detecting() || !agent()}
									onClick={() => createSession(repo)}
								>
									<span class={styles.repoName}>{repoName(repo)}</span>
									<span class={styles.repoPath}>{repo}</span>
								</button>
							)}
						</For>
					</Show>
				</Show>
			</div>
		</div>
	);
}
