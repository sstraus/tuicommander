import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { createAutomationsStore } from "../../stores/automations";
import { registerModal } from "../../stores/modalStack";
import s from "./AutomationsDialog.module.css";
import type { AutomationAdapter } from "./contract";
import { DefinitionEditor } from "./DefinitionEditor";
import { DetailHistory } from "./DetailHistory";
import { zonedTime } from "./ScheduleFields";
import { automationAdapter } from "./transportAdapter";

export function AutomationsDialog(props: { onClose: () => void; adapter?: AutomationAdapter }) {
	const store = createAutomationsStore(props.adapter ?? automationAdapter);
	const [search, setSearch] = createSignal("");
	const [confirmDelete, setConfirmDelete] = createSignal(false);
	let panel: HTMLDivElement | undefined, searchInput: HTMLInputElement | undefined;
	const priorFocus = document.activeElement;
	registerModal(props.onClose);
	onMount(() => {
		searchInput?.focus();
		void store.refresh();
	});
	onCleanup(() => {
		store.dispose();
		if (priorFocus instanceof HTMLElement && priorFocus.isConnected) priorFocus.focus();
	});
	createEffect(() => {
		store.draft()?.id;
		setConfirmDelete(false);
	});
	const choose = (value: Parameters<typeof store.select>[0]) => {
		store.select(value);
		void store.preview();
		void store.loadHistory();
	};
	const focusTrap = (event: KeyboardEvent) => {
		if (event.key !== "Tab") return;
		const controls = [
			...(panel?.querySelectorAll<HTMLElement>(
				'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]',
			) ?? []),
		];
		const first = controls[0],
			last = controls.at(-1);
		if (event.shiftKey && document.activeElement === first) {
			event.preventDefault();
			last?.focus();
		} else if (!event.shiftKey && document.activeElement === last) {
			event.preventDefault();
			first?.focus();
		}
	};
	return (
		<div
			class={s.overlay}
			onClick={(event) => {
				if (event.target === event.currentTarget) props.onClose();
			}}
		>
			<div
				ref={panel}
				class={s.dialog}
				role="dialog"
				aria-modal="true"
				aria-labelledby="automations-title"
				onKeyDown={focusTrap}
			>
				<header class={s.header}>
					<div>
						<h2 id="automations-title">{t("automations.title", "Automations")}</h2>
						<p>{t("automations.machine", "Scheduled agent runs on this machine")}</p>
					</div>
					<button type="button" aria-label={t("automations.close", "Close automations")} onClick={props.onClose}>
						<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
							<path d="m3 2 5 5 5-5 1 1-5 5 5 5-1 1-5-5-5 5-1-1 5-5-5-5z" />
						</svg>
					</button>
				</header>
				<Show when={store.error()}>
					<div role="alert" class={s.error}>
						{store.error()}
					</div>
				</Show>
				<Show when={store.notice()}>
					<div role="status" class={s.notice}>
						{store.notice()}
					</div>
				</Show>
				<div class={s.content}>
					<aside class={s.sidebar} aria-label={t("automations.list", "Automation definitions")}>
						<div class={s.row}>
							<button type="button" disabled={store.busy()} onClick={() => store.create()}>
								{t("automations.new", "New automation")}
							</button>
							<button type="button" disabled={store.busy() || store.loading()} onClick={() => void store.refresh()}>
								{t("automations.refresh", "Refresh")}
							</button>
						</div>
						<input
							ref={searchInput}
							aria-label={t("automations.search", "Search automations")}
							placeholder={t("automations.search", "Search automations")}
							value={search()}
							onInput={(event) => setSearch(event.currentTarget.value)}
						/>
						<Show when={store.loading()}>
							<p role="status">{t("automations.loading", "Loading automations…")}</p>
						</Show>
						<Show when={!store.loading() && !store.error() && store.items().length === 0}>
							<p class={s.empty}>
								{t("automations.empty", "No automations yet. Create one to schedule an agent run.")}
							</p>
						</Show>
						<For
							each={store
								.items()
								.filter((item) =>
									`${item.definition.name} ${item.definition.repository}`
										.toLocaleLowerCase()
										.includes(search().toLocaleLowerCase()),
								)}
						>
							{(item) => (
								<button
									type="button"
									disabled={store.busy()}
									class={s.item}
									aria-pressed={store.draft()?.id === item.definition.id}
									onClick={() => choose(item.definition)}
								>
									<span class={s.itemTitle}>
										{item.definition.name}
										<small>
											{item.definition.enabled ? t("automations.active", "Active") : t("automations.paused", "Paused")}
										</small>
									</span>
									<span>{item.definition.repository}</span>
									<code>
										{item.definition.once_local != null
											? `${t("automations.once", "Once")} ${item.definition.once_local}`
											: item.definition.cron}
									</code>
									<span>{item.definition.timezone}</span>
									<Show when={item.next_run_ms !== null}>
										<time>{zonedTime(item.next_run_ms as number, item.definition.timezone)}</time>
									</Show>
									<Show when={item.last_status}>
										<strong>{item.last_status}</strong>
									</Show>
								</button>
							)}
						</For>
						<Show
							when={
								store.items().length > 0 &&
								!store
									.items()
									.some((item) =>
										`${item.definition.name} ${item.definition.repository}`
											.toLocaleLowerCase()
											.includes(search().toLocaleLowerCase()),
									)
							}
						>
							<p class={s.empty}>{t("automations.noMatches", "No matching automations.")}</p>
						</Show>
					</aside>
					<main class={s.detail}>
						<Show
							when={store.draft()}
							fallback={<p class={s.empty}>{t("automations.select", "Select an automation or create one.")}</p>}
						>
							<div class={s.actions}>
								<Show when={!store.isNew()}>
									<button
										type="button"
										disabled={store.busy()}
										onClick={() => void store.setEnabled(!store.draft()?.enabled)}
									>
										{store.draft()?.enabled ? t("automations.pause", "Pause") : t("automations.resume", "Resume")}
									</button>
									<button type="button" disabled={store.busy()} onClick={() => void store.runNow()}>
										{t("automations.runNow", "Run now")}
									</button>
								</Show>
								<button type="button" class={s.primary} disabled={store.busy()} onClick={() => void store.save()}>
									{t("automations.save", "Save")}
								</button>
								<Show when={!store.isNew()}>
									<button type="button" class={s.danger} disabled={store.busy()} onClick={() => setConfirmDelete(true)}>
										{t("automations.delete", "Delete")}
									</button>
								</Show>
							</div>
							<Show when={confirmDelete()}>
								<div class={s.confirm} role="group" aria-label={t("automations.confirmDelete", "Confirm delete")}>
									<p>{t("automations.deleteHint", "Delete this automation? Saved run history is kept.")}</p>
									<button type="button" class={s.danger} disabled={store.busy()} onClick={() => void store.remove()}>
										{t("automations.confirmDelete", "Confirm delete")}
									</button>
									<button type="button" onClick={() => setConfirmDelete(false)}>
										{t("automations.cancel", "Cancel")}
									</button>
								</div>
							</Show>
							<Show when={store.draft()?.id} keyed>
								{(_id) => <DefinitionEditor store={store} />}
							</Show>
							<Show when={!store.isNew()}>
								<DetailHistory store={store} />
							</Show>
						</Show>
					</main>
				</div>
			</div>
		</div>
	);
}
