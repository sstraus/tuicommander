import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import { Button } from "../components/ui/Button";
import styles from "./backendSkew.module.css";

interface BuildIdentity {
	exe: string;
	build_exe?: string;
	bridge_exe?: string;
	started_at_ms: number;
	git_hash: string;
}

interface HotUpdates {
	on(event: "vite:afterUpdate", callback: () => void): void;
	off(event: "vite:afterUpdate", callback: () => void): void;
}

declare global {
	interface Window {
		__TUIC_DEV_BACKEND__?: BuildIdentity;
	}
}

/** Only imported by the development entry point; never restarts the backend. */
export function startBackendSkewWarning(
	backend = window.__TUIC_DEV_BACKEND__,
	hot: HotUpdates | undefined = import.meta.hot,
): () => void {
	const container = document.createElement("div");
	document.body.append(container);
	const [warning, setWarning] = createSignal("");
	let dismissed = "";
	let currentKey = "";
	let generation = 0;
	let stopped = false;
	const dispose = render(
		() => (
			<Show when={warning()}>
				<div role="alert" class={styles.warning}>
					<span>{warning()}</span>
					<Button
						size="compact"
						variant="ghost"
						shape="icon"
						aria-label="Dismiss backend restart warning"
						onClick={() => {
							dismissed = currentKey;
							setWarning("");
						}}
					>
						×
					</Button>
				</div>
			</Show>
		),
		container,
	);

	async function check() {
		const request = ++generation;
		try {
			const sourceOrigin = new URL(import.meta.url).origin;
			const query = encodeURIComponent(JSON.stringify(backend ?? null));
			const response = await fetch(`${sourceOrigin}/__tuic_backend_identity?backend=${query}`, { cache: "no-store" });
			if (!response.ok) return;
			const tree = (await response.json()) as {
				status: "current" | "stale" | "unavailable";
				key: string;
				backend_id?: string;
				tree_id?: string;
			};
			if (stopped || request !== generation || !tree.key) return;
			if (tree.status === "current") {
				dismissed = "";
				setWarning("");
				return;
			}
			currentKey = tree.key;
			if (dismissed === currentKey) return;
			setWarning(
				tree.status === "stale"
					? `Backend ${tree.backend_id} · tree ${tree.tree_id} · Restart tauri dev`
					: "Backend identity unavailable · Restart tauri dev once",
			);
		} catch {
			// A stopped/restarting Vite server is not evidence of backend skew.
		}
	}
	const onUpdate = () => {
		void check();
	};
	hot?.on("vite:afterUpdate", onUpdate);
	void check();
	const stop = () => {
		stopped = true;
		hot?.off("vite:afterUpdate", onUpdate);
		dispose();
		container.remove();
	};
	import.meta.hot?.dispose(stop);
	return stop;
}
