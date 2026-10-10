import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import { HttpRpcError } from "../../../transport";
import { SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import telegram from "./TelegramTab.module.css";

interface TelegramSettings {
	enabled: boolean;
	token_set: boolean;
	bot_alias: string;
	registered_agent_name: string | null;
	chats: string[];
	connected: boolean;
	polling_owner: "this_app" | "tuic_remote" | "desktop" | "another_process" | null;
	last_error: string | null;
	last_message_time: number | null;
}
function connectionLabel(data: TelegramSettings): string {
	if (!data.enabled) return t("telegram.disabled", "Disabled — enable Telegram to start polling.");
	if (!data.token_set) return t("telegram.missingToken", "No bot token — save and check a token first.");
	if (data.last_error)
		return t(
			"telegram.connectionError",
			"Polling stopped or retrying. Resolve the error below, then disable and enable Telegram.",
		);
	if (!data.connected)
		return t(
			"telegram.disconnected",
			"No poller connected. Check the bot token and enable Telegram in this app or tuic-remote.",
		);
	switch (data.polling_owner) {
		case "this_app":
			return t("telegram.connectedHere", "Connected (this app)");
		case "tuic_remote":
			return t("telegram.connectedRemote", "Connected (tuic-remote)");
		case "desktop":
			return t("telegram.connectedDesktop", "Connected (desktop app)");
		default:
			return t("telegram.connectedOther", "Connected (another process)");
	}
}

export const TelegramTab: Component = () => {
	const [settings, setSettings] = createSignal<TelegramSettings>();
	const [token, setToken] = createSignal("");
	const [chat, setChat] = createSignal("");
	const [code, setCode] = createSignal("");
	const [error, setError] = createSignal("");
	const [busy, setBusy] = createSignal(false);
	let disposed = false;
	let timer: ReturnType<typeof setInterval> | undefined;
	async function refresh() {
		try {
			const result = await invoke<TelegramSettings>("telegram_settings");
			if (!disposed) setSettings(result);
		} catch {
			if (!disposed) setError("Telegram settings unavailable");
		}
	}
	async function change(value: Record<string, unknown>) {
		setBusy(true);
		setError("");
		try {
			const result = await invoke<{ code?: string }>("telegram_setup", { change: value });
			if (result.code) setCode(result.code);
			await refresh();
		} catch (error) {
			const detail = error instanceof HttpRpcError ? error.detail : typeof error === "string" ? error : "";
			setError(detail.startsWith("telegram_") ? detail : "Telegram setup failed. Check the token and chat ID.");
		} finally {
			setBusy(false);
		}
	}
	onMount(() => {
		void refresh();
		timer = setInterval(() => {
			if (!busy()) void refresh();
		}, 3000);
	});
	onCleanup(() => {
		disposed = true;
		clearInterval(timer);
	});
	return (
		<div class={`${s.section} ${telegram.panel}`}>
			<h3>Telegram</h3>
			<p class={s.hint}>
				{t(
					"telegram.pollingHint",
					"This app and tuic-remote share one polling owner on this machine. The other process stays on standby.",
				)}
			</p>
			<Show when={settings()}>
				{(data) => (
					<>
						<div class={s.group}>
							<label for="telegram-token">{t("telegram.token", "Bot token")}</label>
							<p class={s.hint}>{data().token_set ? "Token set — enter a replacement" : "Token not set"}</p>
							<input
								id="telegram-token"
								class={s.input}
								type="password"
								autocomplete="new-password"
								value={token()}
								onInput={(e) => setToken(e.currentTarget.value)}
							/>
							<button
								class={s.testBtn}
								disabled={busy() || !token()}
								onClick={() => {
									const value = token();
									setToken("");
									void change({ action: "token", token: value });
								}}
							>
								Save and check bot
							</button>
							<p class={s.hint}>{data().bot_alias}</p>
						</div>
						<div class={s.group}>
							<label for="telegram-chat">{t("telegram.chat", "Authorized chats")}</label>
							<input
								id="telegram-chat"
								class={s.input}
								inputmode="numeric"
								placeholder="Private chat ID"
								value={chat()}
								onInput={(e) => setChat(e.currentTarget.value)}
							/>
							<button
								class={s.testBtn}
								disabled={busy() || !chat()}
								onClick={() => void change({ action: "add_chat", chat_id: chat() })}
							>
								Add chat ID
							</button>
							<For each={data().chats}>
								{(id) => (
									<p>
										{id}{" "}
										<button
											class={s.testBtn}
											disabled={busy()}
											onClick={() => void change({ action: "remove_chat", chat_id: id })}
										>
											Remove
										</button>
									</p>
								)}
							</For>
							<button
								class={s.testBtn}
								disabled={busy() || !data().enabled || !data().connected}
								onClick={() => void change({ action: "pair" })}
							>
								Link chat
							</button>
							<Show when={code() && data().enabled && data().connected}>
								<p class={s.hint}>
									Send {code()} to the bot. One use, valid for 10 minutes. A bare /start does not authorize a chat.
								</p>
							</Show>
							<Show when={!data().connected}>
								<p class={s.hint}>
									{t(
										"telegram.pairingUnavailable",
										"Connect a poller before linking a chat. Check the connection status below.",
									)}
								</p>
							</Show>
						</div>
						<p class={s.hint} aria-live="polite">
							{data().registered_agent_name
								? `registered agent: ${data().registered_agent_name}`
								: t("telegram.noAgent", "No agent registered")}
						</p>
						<p class={s.hint}>
							{t(
								"telegram.registrationHint",
								"Ask an agent in the polling app to call the Telegram MCP tool with action register. Registration lasts until the agent or polling app exits.",
							)}
						</p>
						<SettingToggle
							label={t("telegram.enabled", "Enable Telegram")}
							checked={data().enabled}
							onChange={(enabled) => {
								if (!busy()) void change({ action: "configure", enabled });
							}}
						/>
						<p role="status" class={s.hint}>
							{connectionLabel(data())}
						</p>
						<Show when={data().last_error}>
							<p class={s.warning}>{data().last_error}</p>
						</Show>
						<Show when={data().last_message_time}>
							<p class={s.hint}>Last message: {new Date(data().last_message_time ?? 0).toLocaleString()}</p>
						</Show>
					</>
				)}
			</Show>
			<Show when={error()}>
				<p role="alert" class={s.warning}>
					{error()}
				</p>
			</Show>
		</div>
	);
};
