/** Isolated presentation fixture: no app bootstrap, transport or persisted settings. */
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import commander from "../../../../src-tauri/src/themes/commander.json";
import paper from "../../../../src-tauri/src/themes/vscode-light.json";
import { Button } from "../../ui/Button";
import "../../../global.css";
import { createMemo } from "solid-js";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { StatusBadge } from "../../ui/StatusBadge";
import s from "../Settings.module.css";
import {
	DangerZone,
	EmptyState,
	ListRow,
	SectionCard,
	SettingPageHeader,
	SettingRow,
	StatusSummary,
} from "../SettingsPrimitives";
import { SettingsShell } from "../SettingsShell";

const theme = new URLSearchParams(location.search).get("theme") === "light" ? paper : commander;
const tokens: Record<string, string> = {
	background: "bg-primary",
	surface: "bg-secondary",
	surfaceElevated: "bg-tertiary",
	highlight: "bg-highlight",
	foreground: "fg-primary",
	foregroundSecondary: "fg-secondary",
	mutedForeground: "fg-muted",
	accentHover: "accent-hover",
	accentForeground: "text-on-accent",
	errorForeground: "text-on-error",
};
for (const [key, value] of Object.entries(theme.appChrome))
	document.documentElement.style.setProperty(`--${tokens[key] ?? key}`, String(value));
document.body.style.margin = "0";
function Preview() {
	const [language, setLanguage] = createSignal("en");
	const [query, setQuery] = createSignal("");
	const tabs = createMemo(() =>
		[
			{ key: "__label__:app", label: "Application" },
			{ key: "general", label: "General" },
			{ key: "appearance", label: "Appearance" },
			{ key: "notifications", label: "Notifications" },
			{ key: "__label__:workspace", label: "Workspace" },
			{ key: "terminal", label: "Terminal" },
		].filter((tab) => tab.key.startsWith("__label__:") || tab.label.toLowerCase().includes(query().toLowerCase())),
	);
	return (
		<SettingsShell
			visible
			onClose={() => {}}
			title="Settings"
			tabs={tabs()}
			navHeader={
				<Input
					aria-label="Find a page"
					placeholder="Find a page..."
					value={query()}
					onInput={(event) => setQuery(event.currentTarget.value)}
				/>
			}
			activeTab="general"
			onTabChange={() => {}}
		>
			<SettingPageHeader
				title="General"
				breadcrumb="Application / General"
				description="Make this workspace feel right. Changes keep their existing scope."
				scope="Application defaults"
				source="Isolated fixture · not live host state"
			/>
			<SectionCard title="General">
				<SettingRow label="Language" hint="Language of the TUICommander interface">
					{(id, hint) => (
						<Select
							id={id}
							aria-describedby={hint()}
							value={language()}
							onChange={(event) => setLanguage(event.currentTarget.value)}
						>
							<option value="en">English</option>
							<option value="it">Italiano</option>
						</Select>
					)}
				</SettingRow>
				<SettingRow
					label="Custom launcher"
					hint="Long paths wrap without hiding the control or its keyboard label."
					help="Define executable arguments with repository and file placeholders. One argument per line."
					error="Illustrative validation message; draft is retained."
				>
					{(id, hint) => <Input id={id} aria-describedby={hint()} value="/workspace/tools/editor" />}
				</SettingRow>
			</SectionCard>
			<SectionCard title="Local tools">
				<ListRow
					name="TUIC CLI"
					description="Control TUIC from a terminal or script."
					badges={<StatusBadge label="Host state not checked" />}
					actions={<Button class={s.textBtn}>Manage CLI</Button>}
				/>
				<ListRow
					name="Code intelligence · MDKB"
					description="Editor definitions, references, symbol outlines and persistent agent memory."
					actions={<Button class={s.textBtn}>Manage index</Button>}
				/>
			</SectionCard>
			<StatusSummary label="Status not available" source="Illustrative fixture" freshness="Not checked">
				<Button class={s.textBtn}>Reload status</Button>
			</StatusSummary>
			<EmptyState
				title="No custom launchers"
				description="Add an installed editor when discovery does not list it."
				action={<Button class={s.textBtn}>Add launcher</Button>}
			/>
			<DangerZone
				title="Disconnect example"
				scope="Illustrative connection only"
				consequence="Connected clients would lose access. No action runs in this fixture."
			>
				<Button class={s.textBtn}>Review disconnect</Button>
			</DangerZone>
		</SettingsShell>
	);
}
render(() => <Preview />, document.getElementById("preview")!);
