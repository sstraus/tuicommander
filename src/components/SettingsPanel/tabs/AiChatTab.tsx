import { type Component, createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { t } from "../../../i18n";
import { type EgoCliClient, egoCli } from "../../../services/egoCli";
import { startEgoLogin } from "../../../services/egoLogin";
import { settingsRefresh } from "../../../services/settingsRefresh";
import { appLogger } from "../../../stores/appLogger";
import { toastsStore } from "../../../stores/toasts";
import type { EgoCliError, EgoCredential, EgoProviders } from "../../../types/ego";
import { asEgoCliError } from "../../../types/ego";
import { Button } from "../../ui/Button";
import { Select } from "../../ui/Select";
import s from "../Settings.module.css";
import { ListRow, SectionCard, SettingPageHeader, SettingRow } from "../SettingsPrimitives";
import layout from "./AiChatTab.module.css";
import { EgoPerimeterSection } from "./EgoPerimeterSection";

/**
 * Which model ego runs by default, and whether each provider can be used.
 *
 * The ego executable itself is a TUICommander setting and lives on General,
 * next to MDKB. Everything on this tab comes from ego's own command line and
 * goes back to it. TUICommander stores no API key, keeps nothing in its
 * keyring, and speaks to no provider: the one network call anywhere near this
 * tab is `ego models --refresh`, which ego makes, and only when a person
 * presses Refresh.
 *
 * Adding a credential does not happen in this form either. `ego auth login` is
 * an interactive flow — a browser round trip, a device code or an API key
 * prompt — so Login opens a terminal tab that runs it and steps aside: the
 * person talks to ego directly and no secret passes through here.
 *
 * The in-chat model switch is a different thing and stays where it is: that one
 * is an ACP session option, which changes one conversation. This changes the
 * default every new run starts from.
 */
export const AiChatTab: Component<{
	client?: EgoCliClient;
	/** Runs `ego auth login PROVIDER` in a terminal tab; `onExit` fires once it ended. */
	startLogin?: typeof startEgoLogin;
	/** Closes Settings so the login terminal is visible. */
	onClose?: () => void;
}> = (props) => {
	const client = () => props.client ?? egoCli;

	const [data, setData] = createSignal<EgoProviders | null>(null);
	const [error, setError] = createSignal<EgoCliError | null>(null);
	const [busy, setBusy] = createSignal(false);
	const [loginError, setLoginError] = createSignal<string | null>(null);

	/** Run one ego call, keeping whatever it printed when it fails. */
	const attempt = async (call: () => Promise<EgoProviders>): Promise<void> => {
		setBusy(true);
		try {
			setData(await call());
			setError(null);
		} catch (err) {
			// An ego failure is rendered with its own words. Anything else is a
			// transport fault and is reported as one rather than dressed up as a
			// refusal ego never made.
			const egoError = asEgoCliError(err);
			if (egoError) {
				setError(egoError);
			} else {
				appLogger.error("config", "ego command failed without an ego error", err);
				setError({
					code: "commandFailed",
					message: String(err),
					command: "",
					stdout: "",
					stderr: "",
					exitCode: null,
				});
			}
		} finally {
			setBusy(false);
		}
	};

	onCleanup(
		settingsRefresh.register({
			section: "ego",
			read: () => client().providers(false),
			apply: (snapshot) => {
				setData(snapshot);
				setError(null);
			},
		}),
	);

	onMount(() => {
		void attempt(() => client().providers(false));
	});

	const login = (provider: string) => {
		try {
			(props.startLogin ?? startEgoLogin)(provider, () => {
				toastsStore.add("ego login", `ego auth login ${provider} finished`, "info");
				void attempt(() => client().providers(false));
			});
			setLoginError(null);
			props.onClose?.();
		} catch (err) {
			setLoginError(err instanceof Error ? err.message : String(err));
		}
	};

	const choose = (slug: string) => {
		if (!slug || slug === data()?.defaultModel) return;
		void attempt(() => client().setDefaultModel(slug));
	};

	/** Every provider that offers at least one model, for the picker. */
	const offering = () => (data()?.providers ?? []).filter((provider) => provider.models.length > 0);

	const empty = () => data() !== null && (data()?.providers.length ?? 0) === 0;

	return (
		<div class={`${s.section} ${layout.page}`}>
			<SettingPageHeader
				title="AI Chat"
				description="Agent defaults and access for new ego runs. Running conversations are unchanged."
				breadcrumb="AI / AI Chat"
			/>
			<SectionCard title="Agent and model">
				<Switch>
					{/* No ego binary is named at all. The fix is on another page, so say
					    which one rather than leaving an empty list on screen. */}
					<Match when={error()?.code === "notConfigured"}>
						<p class={s.warning}>
							{t("providers.unconfigured", "ego is not configured. Set the ego executable in Settings → General.")}
						</p>
					</Match>

					{/* A binary is named and could not be started. That is a different
					    fix from a missing binary, and naming the wrong one wastes a person's
					    time, so the two are never collapsed into "ego is missing". */}
					<Match when={error()?.code === "launchFailed"}>
						<p class={s.warning}>
							{t(
								"providers.launchFailed",
								"The configured ego executable could not be started. Check the path in Settings → General.",
							)}
						</p>
						<pre class={s.mcpSnippetPre}>{error()?.message}</pre>
					</Match>

					<Match when={error()}>
						{(failure) => (
							<>
								<p class={s.warning}>{failure().message}</p>
								<Show when={failure().command}>
									<p class={s.hint}>
										<code class={s.mcpCode}>{failure().command}</code>
										<Show when={failure().exitCode !== null}>
											{" "}
											{t("providers.exitCode", "exited")} {failure().exitCode}
										</Show>
									</p>
								</Show>
								{/* What ego printed, verbatim. A summary here would be this
								    host's opinion about a failure it did not have. */}
								<Show when={failure().stderr || failure().stdout}>
									<pre class={s.mcpSnippetPre}>{failure().stderr || failure().stdout}</pre>
								</Show>
							</>
						)}
					</Match>

					<Match when={empty()}>
						<p class={s.warning}>
							{t(
								"providers.noProviders",
								"ego knows no providers yet. Run ego auth login PROVIDER in a terminal to add a credential, then press Refresh.",
							)}
						</p>
					</Match>

					<Match when={data()}>
						{(loaded) => (
							<SettingRow label={t("providers.label.defaultModel", "Default model")}>
								{(id, descriptionId) => (
									<div class={layout.modelControls}>
										<Select
											data-setting-id="ego.model"
											id={id}
											aria-describedby={descriptionId()}
											disabled={busy()}
											value={loaded().defaultModel ?? ""}
											onChange={(event) => choose(event.currentTarget.value)}
										>
											{/* Present when ego has no `model` key set, so the picker
									    shows "nothing chosen" instead of silently claiming the
									    first model in the list is the default. */}
											<Show when={loaded().defaultModel === null}>
												<option value="">{t("providers.noDefault", "None — ego picks its own")}</option>
											</Show>
											<For each={offering()}>
												{(provider) => (
													<optgroup label={provider.name}>
														<For each={provider.models}>
															{(model) => (
																<option value={model.slug} disabled={!model.available}>
																	{model.name}
																</option>
															)}
														</For>
													</optgroup>
												)}
											</For>
										</Select>
										<Button
											data-non-setting="Explicit configured-Ego provider discovery; never runs during describe"
											class={s.testBtn}
											disabled={busy()}
											onClick={() => void attempt(() => client().providers(true))}
										>
											{t("providers.refresh", "Refresh from providers")}
										</Button>
									</div>
								)}
							</SettingRow>
						)}
					</Match>
				</Switch>

				<Show when={!!error() || !data() || empty()}>
					<SettingRow label="Provider catalogue">
						{() => (
							<Button
								data-non-setting="Explicit configured-Ego provider discovery; never runs during describe"
								class={s.testBtn}
								disabled={busy()}
								onClick={() => void attempt(() => client().providers(true))}
							>
								{t("providers.refresh", "Refresh from providers")}
							</Button>
						)}
					</SettingRow>
				</Show>
				<Show when={(data()?.providers.length ?? 0) > 0}>
					<details>
						<summary>Other providers and available models</summary>
						<Show when={loginError()}>{(message) => <p class={s.warning}>{message()}</p>}</Show>
						<For each={data()?.providers}>
							{(provider) => (
								<div class={s.group}>
									<ListRow
										name={provider.name}
										badges={<CredentialBadge credential={provider.credential} />}
										actions={
											<Button
												data-non-setting="Open the configured Ego interactive login; the human handles authentication"
												class={s.testBtn}
												aria-label={`${t("providers.login", "Login")} to ${provider.name}`}
												onClick={() => login(provider.name)}
											>
												{t("providers.login", "Login")}
											</Button>
										}
									/>
									<p class={s.availabilityDetail}>
										<Show
											when={provider.models.length > 0}
											fallback={t("providers.noModels", "No models — ego could not enumerate this source.")}
										>
											{provider.models.filter((model) => model.available).length}
											{" / "}
											{provider.models.length} {t("providers.modelsUsable", "models usable")}
										</Show>
									</p>
									{/* Why a model cannot be picked, in ego's words and only once
								    per distinct reason: a source that is down says the same
								    sentence about every model it offers. */}
									<For each={reasons(provider.models)}>{(reason) => <p class={s.availabilityDetail}>{reason}</p>}</For>
									{/* What doctor said about this provider. The badge is our word
								    for the state; this is ego's word for the case — which is the
								    only place the difference between "expired at 12:04" and
								    "the store is locked" survives. */}
									<Show when={credentialDetail(provider.credential)}>
										{(detail) => <p class={s.availabilityDetail}>{detail()}</p>}
									</Show>
								</div>
							)}
						</For>
					</details>
				</Show>
			</SectionCard>
			<EgoPerimeterSection />
		</div>
	);
};

/** What `ego doctor` said about this credential, when it said anything.
 *
 * `missing` carries none by construction: there is no report to make about a
 * credential that is not there. */
function credentialDetail(credential: EgoCredential): string | undefined {
	if (credential.state === "missing") return undefined;
	return credential.detail || undefined;
}

/** The distinct reasons a provider's models are unusable, in first-seen order. */
function reasons(models: { available: boolean; unavailable: string | null }[]): string[] {
	const seen: string[] = [];
	for (const model of models) {
		if (model.available || !model.unavailable) continue;
		if (!seen.includes(model.unavailable)) seen.push(model.unavailable);
	}
	return seen;
}

const CredentialBadge: Component<{ credential: EgoCredential }> = (props) => (
	<Switch>
		<Match when={props.credential.state === "stored"}>
			<span class={s.availabilityOk}>{t("providers.credential.stored", "credential stored")}</span>
		</Match>
		{/* Expired is not a call to action: ego refreshes it on its next run, and
		    telling a person to log in again would send them to redo work. */}
		<Match when={props.credential.state === "expired"}>
			<span class={s.availabilityBad}>
				{t("providers.credential.expired", "credential expired — ego refreshes it on its next run")}
			</span>
		</Match>
		<Match when={props.credential.state === "missing"}>
			<span class={s.availabilityBad}>{t("providers.credential.missing", "no credential — use Login")}</span>
		</Match>
		<Match when={props.credential.state === "unknown"}>
			<span class={s.hint}>{t("providers.credential.unknown", "ego could not read its credential store")}</span>
		</Match>
	</Switch>
);
