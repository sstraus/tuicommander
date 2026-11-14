import { createSignal } from "solid-js";
import type {
	AutomationAdapter,
	AutomationDefinition,
	AutomationListItem,
	AutomationRun,
	DefinitionSchedulePreview,
	RunSummary,
	SchedulePreset,
	SchedulePreview,
} from "../components/AutomationsDialog/contract";
import { randomId } from "../utils/randomId";

const [requestedDefinition, setRequestedDefinition] = createSignal<string | null>(null);
const [requestedRun, setRequestedRun] = createSignal<string | null>(null);
const [visible, setVisible] = createSignal(false);
export const automationsUi = {
	visible,
	requestedRun,
	requestedDefinition,
	openDefinition: (id: string) => {
		setRequestedRun(null);
		setRequestedDefinition(id);
		setVisible(true);
	},
	open: () => {
		setRequestedDefinition(null);
		setRequestedRun(null);
		setVisible(true);
	},
	openRun: (id: string) => {
		setRequestedDefinition(null);
		setRequestedRun(id);
		setVisible(true);
	},
	close: () => setVisible(false),
};

/** Dialog-scoped state. The adapter owns validation, schedule interpretation and launch. */
export function createAutomationsStore(adapter: AutomationAdapter) {
	const [items, setItems] = createSignal<AutomationListItem[]>([]);
	const [draft, setDraft] = createSignal<AutomationDefinition>();
	const [isNew, setIsNew] = createSignal(false);
	const [loading, setLoading] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [notice, setNotice] = createSignal("");
	const [runs, setRuns] = createSignal<AutomationRun[]>([]);
	const [historyLoading, setHistoryLoading] = createSignal(false);
	const [schedulePreview, setSchedulePreview] = createSignal<SchedulePreview | DefinitionSchedulePreview>();
	const [previewError, setPreviewError] = createSignal("");
	const [previewLoading, setPreviewLoading] = createSignal(false);
	const [allRuns, setAllRuns] = createSignal<AutomationRun[]>([]);
	const [summaries, setSummaries] = createSignal<Record<string, RunSummary>>({});
	const [runsLoading, setRunsLoading] = createSignal(false);
	const [runsOffset, setRunsOffset] = createSignal(0);
	const [hasMoreRuns, setHasMoreRuns] = createSignal(false);
	let runsEpoch = 0;
	const pageSize = 20;
	async function loadRuns(offset = 0) {
		const epoch = ++runsEpoch;
		setRunsLoading(true);
		setError("");
		try {
			const [history, day, week] = await Promise.all([
				adapter.history(undefined, offset, pageSize + 1, true),
				adapter.summary("24h"),
				adapter.summary("7d"),
			]);
			if (epoch !== runsEpoch) return;
			setAllRuns(history.slice(0, pageSize));
			setHasMoreRuns(history.length > pageSize);
			setRunsOffset(offset);
			setSummaries({ "24h": day, "7d": week });
		} catch (cause) {
			if (epoch === runsEpoch) setError(message(cause));
		} finally {
			if (epoch === runsEpoch) setRunsLoading(false);
		}
	}
	let listEpoch = 0,
		previewEpoch = 0,
		selectionEpoch = 0,
		historyEpoch = 0;
	const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));
	const invalidatePreview = () => {
		previewEpoch++;
		setSchedulePreview(undefined);
		setPreviewError("");
		setPreviewLoading(false);
	};
	function select(value: AutomationDefinition | undefined, creating = false) {
		selectionEpoch++;
		historyEpoch++;
		invalidatePreview();
		setDraft(structuredClone(value));
		setIsNew(creating);
		setRuns([]);
		setHistoryLoading(false);
		setError("");
		setNotice("");
	}
	function edit(patch: Partial<AutomationDefinition>) {
		const changesSchedule = "cron" in patch || "timezone" in patch || "once_local" in patch;
		setDraft((value) => (value ? { ...value, ...(changesSchedule ? { once_at: null } : {}), ...patch } : value));
		if (changesSchedule) invalidatePreview();
	}
	async function openRun(id: string) {
		const epoch = ++selectionEpoch;
		historyEpoch++;
		invalidatePreview();
		setDraft(undefined);
		setRuns([]);
		setHistoryLoading(false);
		setError("");
		setNotice("");
		try {
			const run = await adapter.run(id);
			const current = await adapter.list();
			if (epoch !== selectionEpoch) return;
			// A run's snapshot is history, never an editable replacement definition.
			select(current.find((item) => item.definition.id === run.definition.id)?.definition);
			setRuns([run]);
		} catch (cause) {
			if (epoch === selectionEpoch) setError(message(cause));
		}
	}
	async function refresh() {
		const epoch = ++listEpoch;
		setLoading(true);
		setError("");
		try {
			const values = await adapter.list();
			if (epoch === listEpoch) setItems(values);
		} catch (cause) {
			if (epoch === listEpoch) setError(message(cause));
		} finally {
			if (epoch === listEpoch) setLoading(false);
		}
	}
	async function mutate(operation: () => Promise<void>) {
		if (busy()) return;
		setBusy(true);
		setError("");
		setNotice("");
		try {
			await operation();
		} catch (cause) {
			setError(message(cause));
		} finally {
			setBusy(false);
		}
	}
	async function preview() {
		const value = draft();
		if (!value) return;
		const epoch = ++previewEpoch;
		setPreviewLoading(true);
		setPreviewError("");
		try {
			const reply =
				value.once_local != null
					? await adapter.previewDefinition(structuredClone(value))
					: await adapter.preview(value.cron, value.timezone);
			if (epoch !== previewEpoch) return;
			setSchedulePreview(reply);
			if (!value.timezone) setDraft((current) => (current ? { ...current, timezone: reply.timezone } : current));
		} catch (cause) {
			if (epoch === previewEpoch) {
				setSchedulePreview(undefined);
				setPreviewError(message(cause));
			}
		} finally {
			if (epoch === previewEpoch) setPreviewLoading(false);
		}
	}
	async function loadHistory() {
		const value = draft();
		if (!value || isNew()) return;
		const epoch = ++historyEpoch;
		setHistoryLoading(true);
		try {
			const reply = await adapter.history(value.id);
			if (epoch === historyEpoch) setRuns(reply);
		} catch (cause) {
			if (epoch === historyEpoch) setError(message(cause));
		} finally {
			if (epoch === historyEpoch) setHistoryLoading(false);
		}
	}
	return {
		profiles: adapter.profiles,
		/** History rows carry an output excerpt; the whole saved record comes from `run`. */
		run: adapter.run,
		async openDefinition(id: string) {
			const epoch = ++selectionEpoch;
			historyEpoch++;
			invalidatePreview();
			setDraft(undefined);
			setRuns([]);
			setHistoryLoading(false);
			setError("");
			try {
				const current = await adapter.list();
				if (epoch !== selectionEpoch) return false;
				setItems(current);
				const definition = current.find((item) => item.definition.id === id)?.definition;
				if (!definition) throw new Error("This automation was deleted. Its run history is read-only.");
				select(definition);
				return true;
			} catch (cause) {
				if (epoch === selectionEpoch) setError(message(cause));
				return false;
			}
		},
		templates: adapter.templates,
		applyTemplate: (id: string) =>
			mutate(async () => {
				const value = draft();
				if (!value || !isNew()) return;
				const epoch = selectionEpoch;
				const reply = await adapter.template(id, value.repository, value.run_config, value.timezone);
				if (epoch !== selectionEpoch || draft() !== value) return;
				select(reply, true);
				await preview();
			}),
		openRun,
		allRuns,
		summaries,
		runsLoading,
		runsOffset,
		hasMoreRuns,
		loadRuns,
		items,
		draft,
		isNew,
		loading,
		busy,
		error,
		notice,
		runs,
		historyLoading,
		schedulePreview,
		previewError,
		previewLoading,
		select,
		edit,
		refresh,
		preview,
		loadHistory,
		create() {
			select(
				{
					id: randomId("automation"),
					name: "",
					prompt: "",
					repository: "",
					run_config: "",
					workspace: { mode: "existing" },
					cron: "0 9 * * *",
					timezone: "",
					enabled: true,
					grace_secs: 43200,
					overlap: "skip",
					max_duration_secs: 3600,
					precheck: null,
				},
				true,
			);
		},
		save: () =>
			mutate(async () => {
				const value = draft();
				if (!value) return;
				const epoch = selectionEpoch;
				const saved = await adapter.save(structuredClone(value), isNew());
				if (epoch === selectionEpoch) {
					select(saved);
					setNotice("Saved");
				}
				await refresh();
			}),
		setEnabled: (enabled: boolean) =>
			mutate(async () => {
				const value = draft();
				if (!value || isNew()) return;
				const epoch = selectionEpoch;
				await adapter.setEnabled(value.id, enabled);
				if (epoch === selectionEpoch) edit({ enabled });
				await refresh();
			}),
		remove: () =>
			mutate(async () => {
				const value = draft();
				if (!value || isNew()) return;
				const epoch = selectionEpoch;
				await adapter.remove(value.id);
				if (epoch === selectionEpoch) {
					selectionEpoch++;
					historyEpoch++;
					invalidatePreview();
					setDraft(undefined);
					setRuns([]);
				}
				await refresh();
			}),
		runNow: () =>
			mutate(async () => {
				const value = draft();
				if (!value || isNew()) return;
				const reply = await adapter.runNow(value.id);
				setNotice(`${reply.status}${reply.reason ? `: ${reply.reason}` : ""}`);
				await refresh();
				await loadHistory();
			}),
		applyPreset: (preset: SchedulePreset) =>
			mutate(async () => {
				const epoch = selectionEpoch;
				const previewAtStart = previewEpoch;
				const reply = await adapter.preset(preset);
				if (epoch !== selectionEpoch || previewAtStart !== previewEpoch) return;
				edit({ cron: reply.cron, once_local: null });
				await preview();
			}),
		dispose() {
			runsEpoch++;
			listEpoch++;
			selectionEpoch++;
			historyEpoch++;
			previewEpoch++;
		},
	};
}
export type AutomationsStore = ReturnType<typeof createAutomationsStore>;
