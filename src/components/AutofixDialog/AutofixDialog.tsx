import { type Component, createSignal, Match, onMount, Switch } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import { Button, Textarea } from "../ui";
import s from "./AutofixDialog.module.css";

/** Backend `get_issue_detail` response. `autofix_prompt` is a pre-built,
 *  injection-safe prompt the agent will be seeded with. */
interface IssueDetail {
	number: number;
	title: string;
	body: string;
	author: string;
	url: string;
	comments: unknown;
	autofix_prompt: string;
}

/** Preview + edit the auto-fix prompt before launching an agent in a worktree.
 *  Fetches the issue detail on mount, prefills an editable textarea with the
 *  backend-built prompt, and hands the (possibly edited) prompt back via
 *  `onConfirm`. Mirrors ChangelogModal for layout/theming. */
export const AutofixDialog: Component<{
	repoPath: string;
	issueNumber: number;
	onConfirm: (editedPrompt: string) => void;
	onClose: () => void;
}> = (props) => {
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);
	const [detail, setDetail] = createSignal<IssueDetail | null>(null);
	const [prompt, setPrompt] = createSignal("");

	onMount(async () => {
		try {
			const result = await invoke<IssueDetail>("get_issue_detail", {
				repoPath: props.repoPath,
				issueNumber: props.issueNumber,
			});
			setDetail(result);
			setPrompt(result.autofix_prompt);
		} catch (e) {
			const msg = String(e);
			setError(msg);
			appLogger.error("github", "get_issue_detail failed", { error: msg });
		} finally {
			setLoading(false);
		}
	});

	const handleStart = () => {
		props.onConfirm(prompt());
		props.onClose();
	};

	// Escape-to-close is handled centrally (stores/modalStack): registering routes
	// Escape to props.onClose AND stops it reaching the terminal underneath.
	registerModal(props.onClose);

	return (
		<div class={s.overlay} onClick={props.onClose}>
			<div class={s.modal} onClick={(e) => e.stopPropagation()}>
				<div class={s.header}>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
						<path d="M11.5 1a3.5 3.5 0 0 0-3.4 4.36L1.7 11.77a1.75 1.75 0 0 0 2.47 2.47l6.41-6.4A3.5 3.5 0 1 0 11.5 1Zm0 1.5a2 2 0 0 1 .53 3.93.75.75 0 0 0-.36.2L4.9 13.4a.25.25 0 0 1-.35 0l-.35-.35a.25.25 0 0 1 0-.35l6.77-6.77a.75.75 0 0 0 .2-.36A2 2 0 0 1 11.5 2.5Z" />
					</svg>
					<span class={s.title}>
						{t("github.autofixTitle", "Auto-fix issue")} #{props.issueNumber}
					</span>
					<Button
						variant="ghost"
						class={s.close}
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
								{t("github.autofixLoading", "Loading issue…")}
							</div>
						</Match>
						<Match when={error()}>
							<div class={s.error}>{error()}</div>
						</Match>
						<Match when={detail()}>
							{(d) => (
								<>
									<div class={s.context}>
										<span class={s.issueNum}>#{d().number}</span>
										<span class={s.issueTitle}>{d().title}</span>
									</div>
									<label class={s.label} for="autofix-prompt">
										{t("github.autofixPromptLabel", "Prompt sent to the agent")}
									</label>
									<Textarea
										rows={10}
										id="autofix-prompt"
										class={s.textarea}
										value={prompt()}
										onInput={(e) => setPrompt(e.currentTarget.value)}
										spellcheck={false}
									/>
								</>
							)}
						</Match>
					</Switch>
				</div>

				<div class={s.footer}>
					<Button variant="primary" onClick={handleStart} disabled={loading() || !!error()}>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M4.5 2.5a.5.5 0 0 0-.76-.43l-2 1.2a.5.5 0 0 0 0 .86l2 1.2A.5.5 0 0 0 4.5 5V3.7A4.5 4.5 0 1 1 3.6 9.4a.75.75 0 1 0-1.45.4A6 6 0 1 0 4.5 3.02V2.5Z" />
						</svg>
						{t("github.autofixStart", "Start auto-fix")}
					</Button>
					<span class={s.spacer} />
					<Button onClick={props.onClose}>{t("common.cancel", "Cancel")}</Button>
				</div>
			</div>
		</div>
	);
};
