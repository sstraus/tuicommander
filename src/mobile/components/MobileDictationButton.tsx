import { Show } from "solid-js";
import { Button } from "../../components/ui";
import { t } from "../../i18n";
import type { useBrowserDictation } from "../useBrowserDictation";
import s from "./CommandInput.module.css";

export function MobileDictationButton(props: {
	dictation: ReturnType<typeof useBrowserDictation>;
	disabled?: boolean;
}) {
	return (
		<Show when={props.dictation.available}>
			<Button
				shape="icon"
				variant="ghost"
				class={s.dictate}
				type="button"
				aria-label={
					props.dictation.recording()
						? t("mobile.dictation.stop", "Stop dictation")
						: t("mobile.dictation.start", "Dictate into draft")
				}
				aria-pressed={props.dictation.recording()}
				data-failed={props.dictation.failed() || undefined}
				disabled={props.disabled}
				onPointerDown={(event) => event.preventDefault()}
				onClick={props.dictation.toggle}
			>
				<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
					<path d="M12 15a3 3 0 0 0 3-3V5a3 3 0 0 0-6 0v7a3 3 0 0 0 3 3Zm5-3a5 5 0 0 1-10 0H5a7 7 0 0 0 6 6.93V22h2v-3.07A7 7 0 0 0 19 12Z" />
				</svg>
			</Button>
			<Show when={props.dictation.failed()}>
				<span class={s.dictationFailure} role="status">
					{t("mobile.dictation.error", "Dictation failed")}
				</span>
			</Show>
		</Show>
	);
}
