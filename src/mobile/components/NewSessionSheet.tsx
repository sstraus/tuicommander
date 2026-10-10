import { createSignal, For, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import styles from "./NewSessionSheet.module.css";

interface NewSessionSheetProps {
	repos: string[];
	onDismiss: () => void;
	onCreated?: (sessionId: string) => void;
}

const AGENTS = [
	{ id: "claude", label: "Claude Code" },
	{ id: "codex", label: "Codex" },
	{ id: "gemini", label: "Gemini" },
] as const;

function repoName(path: string): string {
	const parts = path.split("/");
	return parts.at(-1)! || path;
}

export function NewSessionSheet(props: NewSessionSheetProps) {
	const [agent, setAgent] = createSignal<(typeof AGENTS)[number]["id"]>("claude");
	const [query, setQuery] = createSignal("");
	const [creating, setCreating] = createSignal(false);
	const visibleRepos = () => props.repos.filter((repo) => repo.toLowerCase().includes(query().trim().toLowerCase()));

	async function createSession(cwd: string) {
		if (creating()) return;
		setCreating(true);
		try {
			const sessionId = await rpc<string>("spawn_agent", {
				pty_config: { cwd, rows: 24, cols: 80 },
				agent_config: { cwd, agent_type: agent(), prompt: "", print_mode: false, args: [] },
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
					<button type="button" class={styles.close} aria-label="Close new session" onClick={props.onDismiss}>
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
					</button>
				</div>
				<label class={styles.fieldLabel} for="new-session-agent">
					Agent
				</label>
				<select
					id="new-session-agent"
					class={styles.field}
					value={agent()}
					onChange={(event) => setAgent(event.currentTarget.value as (typeof AGENTS)[number]["id"])}
				>
					<For each={AGENTS}>{(choice) => <option value={choice.id}>{choice.label}</option>}</For>
				</select>
				<label class={styles.fieldLabel} for="new-session-repo">
					Repository
				</label>
				<input
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
								<button class={styles.repoItem} disabled={creating()} onClick={() => createSession(repo)}>
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
