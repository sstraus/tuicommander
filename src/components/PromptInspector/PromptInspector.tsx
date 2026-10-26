import { type Component, createEffect, createResource, For, Show } from "solid-js";
import { invoke } from "../../invoke";
import { registerModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import s from "./PromptInspector.module.css";

export interface PromptReceipt {
	sections: {
		label: string;
		source: string;
		bytes: number | null;
		text: string;
		status: string;
		truncated: boolean;
		count: number;
		firstSeenMs: number;
		lastSeenMs: number;
	}[];
	captureLimited: boolean;
}

/** The backend rejection text (a string over IPC, an Error over HTTP) says why the read failed. */
function errorText(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

export const PromptInspector: Component<{ sessionId: string | null; onClose: () => void }> = (props) => {
	const [receipt] = createResource(
		() => props.sessionId || undefined,
		(sessionId) => invoke<PromptReceipt>("get_prompt_receipt", { sessionId }),
	);
	createEffect(() => {
		if (props.sessionId) registerModal(props.onClose);
	});
	return (
		<Show when={props.sessionId}>
			<div class={d.overlay} onClick={props.onClose}>
				<section
					class={`${d.popover} ${s.dialog}`}
					role="dialog"
					aria-modal="true"
					aria-label="Prompt inspector"
					onClick={(e) => e.stopPropagation()}
				>
					<div class={d.header}>
						<h4>Prompt inspector</h4>
					</div>
					<div class={d.body}>
						<Show when={receipt.loading}>
							<p>Loading launch receipt…</p>
						</Show>
						<Show when={receipt.error}>
							<p role="alert">Could not read the launch receipt: {errorText(receipt.error)}</p>
						</Show>
						<Show when={!receipt.loading && !receipt.error && receipt()}>
							{(data) => (
								<>
									<For each={data().sections}>
										{(section) => (
											<details class={s.section}>
												<summary>
													{section.label}
													<span class={s.size}>
														{section.bytes === null ? "Size unknown" : `${section.bytes} bytes`}
													</span>
												</summary>
												<p class={s.source}>{section.source}</p>
												<p class={s.note}>
													{section.status === "not_observable"
														? "Not observable by TUIC"
														: section.status === "file_snapshot"
															? "File snapshot supplied at launch; the agent’s actual file read is unverified"
															: section.status === "queued"
																? "Queued — not yet sent"
																: section.status === "served"
																	? "Served in MCP initialize"
																	: "Sent at launch"}
												</p>
												<Show when={section.count > 1}>
													<p class={s.note}>
														Served {section.count} times; showing the latest, last seen{" "}
														{new Date(section.lastSeenMs).toLocaleString()}.
													</p>
												</Show>
												<Show when={section.truncated}>
													<p class={s.note}>Truncated redacted preview (32 KiB per section, 64 KiB per receipt).</p>
												</Show>
												<pre class={s.text}>{section.text}</pre>
											</details>
										)}
									</For>
									<Show when={data().captureLimited}>
										<p class={s.note}>Additional sections omitted by the 16-section capture limit.</p>
									</Show>
								</>
							)}
						</Show>
					</div>
					<div class={d.actions}>
						<Button class={d.cancelBtn} onClick={props.onClose}>
							Close
						</Button>
					</div>
				</section>
			</div>
		</Show>
	);
};
