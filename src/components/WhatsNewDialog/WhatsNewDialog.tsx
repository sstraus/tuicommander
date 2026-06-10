import { type Component, createEffect, For, onCleanup, Show } from "solid-js";
import d from "../shared/dialog.module.css";

export interface Contribution {
	text: string;
	author: string;
}

export interface WhatsNewDialogProps {
	visible: boolean;
	version: string;
	highlights: string[];
	contributions: Contribution[];
	onClose: () => void;
}

export const WhatsNewDialog: Component<WhatsNewDialogProps> = (props) => {
	createEffect(() => {
		if (!props.visible) return;

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Escape" || e.key === "Enter") {
				e.preventDefault();
				props.onClose();
			}
		};

		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} style={{ width: "420px" }} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<h4>What's New in v{props.version}</h4>
					</div>
					<div class={d.body}>
						<ul
							style={{
								margin: 0,
								padding: "0 0 0 18px",
								"list-style": "disc",
								color: "var(--fg-secondary)",
								"font-size": "var(--font-md)",
								display: "flex",
								"flex-direction": "column",
								gap: "6px",
							}}
						>
							<For each={props.highlights}>{(item) => <li>{item}</li>}</For>
							<For each={props.contributions}>
								{(c) => (
									<li>
										{c.text}
										{" — thanks "}
										<a
											href={`https://github.com/${c.author}`}
											target="_blank"
											rel="noopener noreferrer"
											style={{ color: "var(--accent)" }}
										>
											@{c.author}
										</a>
									</li>
								)}
							</For>
						</ul>
						<a
							href="https://github.com/sstraus/tuicommander/blob/main/CHANGELOG.md"
							target="_blank"
							rel="noopener noreferrer"
							style={{
								display: "inline-block",
								"margin-top": "12px",
								"font-size": "var(--font-sm)",
								color: "var(--accent)",
							}}
						>
							View full changelog
						</a>
						<a
							href="https://github.com/sstraus/tuicommander"
							target="_blank"
							rel="noopener noreferrer"
							style={{
								display: "flex",
								"align-items": "center",
								gap: "6px",
								"margin-top": "14px",
								"padding-top": "12px",
								"border-top": "1px solid var(--border)",
								"font-size": "var(--font-sm)",
								color: "var(--fg-secondary)",
								"text-decoration": "none",
							}}
						>
							<svg
								width="14"
								height="14"
								viewBox="0 0 24 24"
								fill="currentColor"
								style={{ color: "var(--accent)", "flex-shrink": 0 }}
								aria-hidden="true"
							>
								<path d="M12 2l2.9 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14l-5-4.87 7.1-1.01L12 2z" />
							</svg>
							Enjoying TUICommander? Star us on GitHub
						</a>
					</div>
					<div class={d.actions}>
						<button class={d.primaryBtn} onClick={props.onClose} style={{ flex: "1" }}>
							Got it
						</button>
					</div>
				</div>
			</div>
		</Show>
	);
};
