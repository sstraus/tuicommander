import { type Component, createSignal, Match, onCleanup, onMount, Switch } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import type { ChangelogResult } from "../../types";
import { writeClipboard } from "../../utils/clipboard";
import { downloadText } from "../../utils/downloadText";
import { Button } from "../ui/Button";
import s from "./ChangelogModal.module.css";

/**
 * Modal that generates and displays an ego-authored changelog for a repo.
 *
 * The error branch shows ego's own sentence rather than a generic one: "no ego
 * executable is configured" tells a person what to do and "could not generate"
 * does not.
 */
export const ChangelogModal: Component<{ repoPath: string; onClose: () => void }> = (props) => {
	const [loading, setLoading] = createSignal(true);
	const [markdown, setMarkdown] = createSignal("");
	const [error, setError] = createSignal<string | null>(null);
	const [copied, setCopied] = createSignal(false);

	// Escape-to-close handled centrally (stores/modalStack): closes this modal and
	// keeps Escape from reaching the terminal underneath.
	registerModal(props.onClose);

	onMount(async () => {
		try {
			const result = await invoke<ChangelogResult>("generate_changelog", { repoPath: props.repoPath });
			setMarkdown(result.markdown);
		} catch (e) {
			const msg = String(e);
			setError(msg);
			appLogger.error("github", "generate_changelog failed", { error: msg });
		} finally {
			setLoading(false);
		}
	});

	const actionsDisabled = () => loading() || markdown().length === 0;

	let copiedTimer: ReturnType<typeof setTimeout> | undefined;
	onCleanup(() => clearTimeout(copiedTimer));

	const handleCopy = async () => {
		try {
			await writeClipboard(markdown());
			setCopied(true);
			clearTimeout(copiedTimer);
			copiedTimer = setTimeout(() => setCopied(false), 1500);
		} catch (e) {
			appLogger.warn("github", "changelog copy failed", { error: String(e) });
		}
	};

	const handleSave = () => {
		downloadText("CHANGELOG-ai.md", markdown());
	};

	return (
		<div class={s.overlay} onClick={props.onClose}>
			<div class={s.modal} onClick={(e) => e.stopPropagation()}>
				<div class={s.header}>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
						<path d="M4 1.5A1.5 1.5 0 0 0 2.5 3v10A1.5 1.5 0 0 0 4 14.5h8a1.5 1.5 0 0 0 1.5-1.5V5.5L9.5 1.5H4Zm5 1.06L12.44 6H9.5a.5.5 0 0 1-.5-.5V2.56ZM5 8h6v1H5V8Zm0 2.5h6v1H5v-1ZM5 5.5h2v1H5v-1Z" />
					</svg>
					<span class={s.title}>{t("github.changelog", "Changelog")}</span>
					<Button
						variant="ghost"
						onClick={props.onClose}
						shape="icon"
						aria-label={t("common.close", "Close")}
						title={t("common.close", "Close")}
					>
						&times;
					</Button>
				</div>

				<div class={s.body}>
					<Switch>
						<Match when={loading()}>
							<div class={s.loading}>
								<span class={s.spinner} />
								{t("github.changelogGenerating", "Generating changelog…")}
							</div>
						</Match>
						<Match when={error()}>
							<div class={s.error}>{error()}</div>
						</Match>
						<Match when={markdown()}>
							<pre class={s.markdown}>{markdown()}</pre>
						</Match>
					</Switch>
				</div>

				<div class={s.footer}>
					<Button onClick={handleCopy} disabled={actionsDisabled()}>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M10 1.5H6a.5.5 0 0 0-.5.5v1H4A1.5 1.5 0 0 0 2.5 4.5v9A1.5 1.5 0 0 0 4 15h6a1.5 1.5 0 0 0 1.5-1.5v-1H13a1.5 1.5 0 0 0 1.5-1.5v-7L10 1.5ZM10.5 12v1.5a.5.5 0 0 1-.5.5H4a.5.5 0 0 1-.5-.5v-9A.5.5 0 0 1 4 4h1.5v6.5A1.5 1.5 0 0 0 7 12h3.5ZM9.5 3.56 12.44 6.5H10a.5.5 0 0 1-.5-.5V3.56Z" />
						</svg>
						{copied() ? t("github.copied", "Copied") : t("github.copy", "Copy")}
					</Button>
					<Button onClick={handleSave} disabled={actionsDisabled()}>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M8 1a.5.5 0 0 1 .5.5v6.79l2.15-2.14a.5.5 0 0 1 .7.7l-3 3a.5.5 0 0 1-.7 0l-3-3a.5.5 0 1 1 .7-.7L7.5 8.29V1.5A.5.5 0 0 1 8 1ZM2.5 11a.5.5 0 0 1 .5.5V13h10v-1.5a.5.5 0 0 1 1 0V13a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-1.5a.5.5 0 0 1 .5-.5Z" />
						</svg>
						{t("github.save", "Save")}
					</Button>
					<span class={s.spacer} />
					<Button onClick={props.onClose}>{t("common.close", "Close")}</Button>
				</div>
			</div>
		</div>
	);
};
