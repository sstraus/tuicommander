import {
	type Accessor,
	type Component,
	children,
	createContext,
	createEffect,
	createSignal,
	type JSX,
	onCleanup,
	Show,
	useContext,
} from "solid-js";
import { t } from "../../i18n";
import { settingsExpertStore } from "../../stores/settingsExpert";
import { uiStore } from "../../stores/ui";
import { Switch } from "../ui/Switch";
import s from "./Settings.module.css";

interface ExpertSectionContext {
	register: (visible: Accessor<boolean>) => void;
}

const SectionContext = createContext<ExpertSectionContext>();

/**
 * A setting the default is right for almost everyone. Hidden in basic mode
 * while `value` equals the default at `configKey`; shown in expert mode, when
 * modified, when a search result revealed it, or while defaults are unknown.
 *
 * `configKey` follows the convention in `stores/settingsExpert.ts` and must be
 * a string literal: the search-index drift test reads it from the source to
 * mark the wrapped labels as expert. `value` is the control's live value in
 * the serialized config shape (snake_case domain, Rust types).
 *
 * A user edit — a native `input` or `change` event from any child control —
 * pins the setting for the rest of the Settings open, so editing it back to
 * the default does not hide it mid-edit.
 */
export const ExpertSetting: Component<{ configKey: string; value: unknown; children: JSX.Element }> = (props) => {
	const visible = () => settingsExpertStore.isVisible(props.configKey, props.value);
	useContext(SectionContext)?.register(visible);
	return (
		<Show when={visible()}>
			<PinOnEdit configKey={props.configKey}>{props.children}</PinOnEdit>
		</Show>
	);
};

const EDIT_EVENTS = ["input", "change"] as const;

/**
 * Pins `configKey` on an edit inside its children. No wrapper element: one
 * would break the `.group + .group` and `:last-child` rules of the Settings
 * CSS. The listeners sit on the resolved top-level elements in the capture
 * phase, so the pin lands before the control's own handler moves the value to
 * the default — the control is never unmounted and keeps focus.
 */
const PinOnEdit: Component<{ configKey: string; children: JSX.Element }> = (props) => {
	const resolved = children(() => props.children);
	const pin = () => settingsExpertStore.pin(props.configKey);
	createEffect(() => {
		const elements = resolved.toArray().filter((node): node is Element => node instanceof Element);
		for (const element of elements) for (const type of EDIT_EVENTS) element.addEventListener(type, pin, true);
		onCleanup(() => {
			for (const element of elements) for (const type of EDIT_EVENTS) element.removeEventListener(type, pin, true);
		});
	});
	return <>{resolved()}</>;
};

/**
 * A settings section (`<div class={s.section}>`) made only of `ExpertSetting`s.
 * It hides itself, heading included, when every one of them is hidden. Use it
 * only when the section holds no basic control — a basic control does not
 * register, so it would be hidden along with the expert ones.
 */
export const ExpertSection: Component<{ children: JSX.Element }> = (props) => {
	const [members, setMembers] = createSignal<Accessor<boolean>[]>([]);
	const register = (visible: Accessor<boolean>) => {
		setMembers((prev) => [...prev, visible]);
		onCleanup(() => setMembers((prev) => prev.filter((member) => member !== visible)));
	};
	// Kept mounted and hidden, not unmounted: the members must stay registered
	// to bring the section back when one of them becomes visible.
	const hidden = () => {
		const all = members();
		return all.length > 0 && all.every((visible) => !visible());
	};
	return (
		<SectionContext.Provider value={{ register }}>
			<div class={s.section} hidden={hidden()}>
				{props.children}
			</div>
		</SectionContext.Provider>
	);
};

/** The Settings header's "Expert" switch — flips the persisted pref. */
export const ExpertModeSwitch: Component = () => (
	<label class={s.expertSwitch}>
		<span>{t("settings.expert.switch", "Expert")}</span>
		<Switch
			data-setting-id="preferences.settings_expert_mode"
			role="switch"
			checked={uiStore.state.settingsExpertMode}
			onChange={(e) => uiStore.setSettingsExpertMode(e.currentTarget.checked)}
		/>
	</label>
);
