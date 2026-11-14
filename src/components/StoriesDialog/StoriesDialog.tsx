import {
	batch,
	type Component,
	createEffect,
	createMemo,
	createSignal,
	createUniqueId,
	For,
	on,
	onCleanup,
	onMount,
	Show,
	Suspense,
	untrack,
} from "solid-js";
import { initMouseDrag } from "../../hooks/useMouseDrag";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import { repositoriesStore } from "../../stores/repositories";
import { type StoriesTarget, storiesUi } from "../../stores/storiesUi";
import { workflowsUi } from "../../stores/workflowsUi";
import { HttpRpcError, isTauri } from "../../transport";
import type { PlanState, StoryStatus, StoryType, StoryVisibility } from "../../types/stories.gen";
import { openFileAction } from "../../utils/filePreview";
import { handleOpenUrl } from "../../utils/openUrl";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import { Textarea } from "../ui/Textarea";
import { type EditablePlan, PlanEditForm, type PlanEditPatch } from "./PlanEditForm";
import s from "./StoriesDialog.module.css";
import { StoryDelivery } from "./StoryDelivery";
import { type EditableStory, StoryEditForm, type StoryEditPatch, type StoryEdits } from "./StoryEditForm";
import { StoryMetadata } from "./StoryMetadata";
import { StoryNotes } from "./StoryNotes";
import { WaitingOnYou } from "./WaitingOnYou";

interface Plan extends EditablePlan {
	archived: boolean;
	project: string;
}
interface PlanSource {
	title: string;
	source: string;
}
interface StoryColumn {
	status: StoryStatus;
	storyIds: string[];
}
interface Story extends EditableStory {
	archived: boolean;
	planId: string;
	checked: boolean[];
	status: StoryStatus;
	claimSession: string | null;
	abandoned: boolean;
	/** Done in its external backend without a TUIC integration receipt. */
	completedOutside?: boolean;
}
interface PhaseMove {
	status: StoryStatus;
	command: string;
}
interface PhaseChoices {
	revision: number;
	moves: PhaseMove[];
	error?: string;
}

interface BackendStatus {
	backend: string;
	label: string;
	provenance: string;
	reason: string;
	availability: string;
	installation: string | null;
	generation?: string | null;
	version: string | null;
	error: string | null;
}

const ALL_PLANS = "all-plans";

type Reply =
	| { type: "backend_status"; value: BackendStatus }
	| { type: "plan"; value: Plan }
	| { type: "plans"; value: Plan[] }
	| { type: "plan_sources"; value: PlanSource[] }
	| { type: "plan_state"; value: PlanState }
	| {
			type: "plan_view";
			value: {
				stories: Story[];
				state: PlanState;
				wontFixCount: number;
				allCancelled: boolean;
				visibleStoryIds?: string[];
				columns?: StoryColumn[];
			};
	  }
	| { type: "project_story_view"; value: { stories: Story[]; visibleStoryIds: string[]; columns: StoryColumn[] } }
	| { type: "story"; value: Story }
	| { type: "stories"; value: Story[] }
	| { type: "available_transitions"; value: PhaseChoices };

const missingBackendMessage = () =>
	t(
		"stories.error.missingBackend",
		"Restart TUICommander to load Plans and Stories. The running app needs a newer backend.",
	);

class MissingStoriesBackendError extends Error {
	constructor() {
		super(missingBackendMessage());
	}
}

/** The backend probe answers with a capability descriptor; anything else is an older backend. */
const isStoryCapabilities = (value: unknown): boolean =>
	typeof value === "object" &&
	value !== null &&
	typeof (value as { backend?: unknown }).backend === "string" &&
	typeof (value as { operations?: unknown }).operations === "object";

const statusLabel = (status: StoryStatus): string => {
	switch (status) {
		case "backlog":
			return t("stories.status.backlog", "Backlog");
		case "ready":
			return t("stories.status.ready", "Ready");
		case "in_progress":
			return t("stories.status.inProgress", "In progress");
		case "review":
			return t("stories.status.review", "Review");
		case "qa":
			return t("stories.status.qa", "QA");
		case "done":
			return t("stories.status.done", "Done");
		case "blocked":
			return t("stories.status.blocked", "Blocked");
		case "wont_fix":
			return t("stories.status.wontFix", "Won't fix");
	}
};

function PhaseSelector(props: {
	story: Story;
	choices: PhaseChoices | undefined;
	disabled: boolean;
	label: string;
	onMove: (story: Story, command: string) => void;
}) {
	const moves = () => (props.choices?.revision === props.story.revision ? props.choices.moves : []);
	return (
		<label class={s.phaseSelector} data-status={props.story.status}>
			<span>{t("stories.phase.label", "Phase")}</span>
			<Select
				aria-label={props.label}
				title={props.choices?.error}
				value=""
				disabled={props.disabled || moves().length === 0}
				onChange={(event) => {
					const command = event.currentTarget.value;
					event.currentTarget.value = "";
					if (moves().some((move) => move.command === command)) props.onMove(props.story, command);
				}}
			>
				<option value="">{statusLabel(props.story.status)}</option>
				<For each={moves()}>{(move) => <option value={move.command}>{statusLabel(move.status)}</option>}</For>
			</Select>
		</label>
	);
}

const storyTypeLabel = (kind: StoryType | null | undefined): string => {
	switch (kind) {
		case "feature":
			return t("stories.type.feature", "Feature");
		case "fix":
			return t("stories.type.fix", "Fix");
		case "refactor":
			return t("stories.type.refactor", "Refactor");
		case "chore":
			return t("stories.type.chore", "Chore");
		case "task":
			return t("stories.type.task", "Task");
		default:
			return t("stories.type.untyped", "Untyped");
	}
};
interface StoryDraft {
	title: string;
	storyType: StoryType;
	priority: number;
	problem: string;
	criteria: string[];
}

const STORY_TYPES: StoryType[] = ["feature", "fix", "refactor", "chore", "task"];

const planStateLabel = (state: string): string => {
	switch (state) {
		case "draft":
			return t("stories.planState.draft", "Draft");
		case "active":
			return t("stories.planState.active", "Active");
		case "done":
			return t("stories.planState.done", "Done");
		default:
			return state;
	}
};

export interface StoriesDialogProps {
	project: string;
	onClose: () => void;
	mode?: "inline" | "detached";
}

export const StoriesDialog: Component<StoriesDialogProps> = (props) => {
	const mode = () => props.mode ?? "inline";
	if (mode() === "inline") registerModal(props.onClose);
	const [plans, setPlans] = createSignal<Plan[]>([]);
	const [planSources, setPlanSources] = createSignal<PlanSource[]>([]);
	const [stories, setStories] = createSignal<Story[]>([]);
	const [visibleIds, setVisibleIds] = createSignal<string[] | undefined>();
	const [columns, setColumns] = createSignal<StoryColumn[]>([]);
	const [phaseChoices, setPhaseChoices] = createSignal<ReadonlyMap<string, PhaseChoices>>(new Map());
	const [draggedStory, setDraggedStory] = createSignal<Story | null>(null);
	const [dropTarget, setDropTarget] = createSignal<StoryStatus | null>(null);
	let boardRoot: HTMLDivElement | undefined;
	let suppressCardClick = false;
	let cancelStoryDrag: (() => void) | undefined;
	onCleanup(() => cancelStoryDrag?.());
	const [archivedPlans, setArchivedPlans] = createSignal(false);
	const [archivedStories, setArchivedStories] = createSignal(false);
	const [newStoryType, setNewStoryType] = createSignal<StoryType | "untyped">("untyped");
	const filter = () => ({
		visibility: storiesUi.visibility(),
		storyType: storiesUi.storyType() === "all" || storiesUi.storyType() === "untyped" ? null : storiesUi.storyType(),
		untypedOnly: storiesUi.storyType() === "untyped",
		...(archivedStories() ? { archived: true } : {}),
	});
	const storiesById = createMemo(() => new Map(stories().map((story) => [story.id, story])));
	const listedStories = () =>
		visibleIds()
			?.map((id) => storiesById().get(id))
			.filter((story): story is Story => !!story) ?? stories();
	const [planId, setPlanId] = createSignal<string | null>(null);
	const [storyId, setStoryId] = createSignal<string | null>(null);
	const [editingPlan, setEditingPlan] = createSignal<Plan | null>(null);
	const [planEditError, setPlanEditError] = createSignal("");
	createEffect(
		on(planId, () => {
			setEditingPlan(null);
			setPlanEditError("");
		}),
	);
	const [editing, setEditing] = createSignal<Story | null>(null);
	const [editError, setEditError] = createSignal("");
	createEffect(
		on(storyId, () => {
			setEditing(null);
			setEditError("");
		}),
	);
	const [planState, setPlanState] = createSignal<string>("draft");
	const [wontFixCount, setWontFixCount] = createSignal(0);
	const [allCancelled, setAllCancelled] = createSignal(false);
	const [loading, setLoading] = createSignal(true);
	createEffect(
		on(
			() => [props.project, planId(), storiesUi.view(), loading()],
			() => cancelStoryDrag?.(),
		),
	);
	const [readFailed, setReadFailed] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [newPlan, setNewPlan] = createSignal(false);
	const [sourcesLoading, setSourcesLoading] = createSignal(false);
	const [newStory, setNewStory] = createSignal(false);
	const [planTitle, setPlanTitle] = createSignal("");
	const [planSource, setPlanSource] = createSignal("");
	const [storyTitle, setStoryTitle] = createSignal("");
	const [storyRequest, setStoryRequest] = createSignal("");
	const [storyProblem, setStoryProblem] = createSignal("");
	const [filling, setFilling] = createSignal(false);
	const [cancellingFill, setCancellingFill] = createSignal(false);
	const [fillError, setFillError] = createSignal("");
	const [fillNotice, setFillNotice] = createSignal("");
	let fillEpoch = 0;
	let fillHandle: { id: string; project: string } | undefined;
	let pendingFill: Promise<StoryDraft> | undefined;

	async function fillStory(): Promise<void> {
		if (filling() || busy() || !storyRequest().trim()) return;
		const epoch = ++fillEpoch;
		const project = props.project;
		const request = storyRequest();
		setFilling(true);
		setFillError("");
		setFillNotice("");
		try {
			const handle = await invoke<string>("acp_one_shot_prepare", { project });
			if (epoch !== fillEpoch || project !== props.project) {
				await invoke("acp_one_shot_cancel", { project, requestId: handle });
				return;
			}
			fillHandle = { id: handle, project };
			pendingFill = invoke<StoryDraft>("story_generate_draft", { project, request, requestId: handle });
			const draft = await pendingFill;
			if (epoch !== fillEpoch || project !== props.project) return;
			batch(() => {
				setStoryTitle(draft.title);
				setNewStoryType(draft.storyType);
				setPriority(draft.priority);
				setStoryProblem(draft.problem);
				setCriteriaText(draft.criteria.join("\n"));
			});
		} catch (cause) {
			if (epoch === fillEpoch)
				setFillError(
					cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause),
				);
		} finally {
			if (epoch === fillEpoch) {
				fillHandle = undefined;
				pendingFill = undefined;
				setFilling(false);
			}
		}
	}

	async function cancelFill(): Promise<void> {
		if (!filling() || cancellingFill()) return;
		++fillEpoch;
		const handle = fillHandle;
		const pending = pendingFill;
		setCancellingFill(true);
		try {
			if (handle) await invoke("acp_one_shot_cancel", { project: handle.project, requestId: handle.id });
			// The generation call settles only after its owned connection is shut down.
			await pending?.catch(() => {});
			setFillNotice(t("stories.ai.cancelled", "Fill cancelled."));
		} catch (cause) {
			setFillError(cause instanceof HttpRpcError ? cause.detail : String(cause));
		} finally {
			fillHandle = undefined;
			pendingFill = undefined;
			setFilling(false);
			setCancellingFill(false);
		}
	}

	onCleanup(() => {
		++fillEpoch;
		if (fillHandle)
			void invoke("acp_one_shot_cancel", { project: fillHandle.project, requestId: fillHandle.id }).catch((cause) => {
				appLogger.warn("store", "Stories: cancel fill failed", { error: String(cause) });
			});
	});
	const [criteriaText, setCriteriaText] = createSignal("");
	const [scopeText, setScopeText] = createSignal("");
	const [priority, setPriority] = createSignal(2);
	const [originType, setOriginType] = createSignal<"native" | "plan_step">("native");
	const [planStep, setPlanStep] = createSignal("");
	const [dependencyId, setDependencyId] = createSignal("");
	const dependencyHintId = createUniqueId();
	const dependencyReason = () => {
		if (busy()) return t("stories.dependencySaving", "Saving changes…");
		if (!dependencyId()) return t("stories.dependencyRequired", "Select a story to add a dependency.");
		return "";
	};
	const cancelDragOnBlur = () => cancelStoryDrag?.();
	onMount(() => window.addEventListener("blur", cancelDragOnBlur));
	onCleanup(() => window.removeEventListener("blur", cancelDragOnBlur));
	let closeButton: HTMLButtonElement | undefined;
	let request = 0;
	onCleanup(() => {
		++request;
	});
	const [backendStatus, setBackendStatus] = createSignal<BackendStatus | null>(null);
	const [capabilitiesReady, setCapabilitiesReady] = createSignal(false);
	// What the project's story backend supports; an external adapter declares fewer operations.
	const [operations, setOperations] = createSignal<Record<string, boolean | StoryEdits>>({});
	const can = (operation: string): boolean =>
		capabilitiesReady() &&
		(!backendStatus() || backendStatus()?.availability === "available") &&
		operations()[operation] !== false;
	const storyEdits = (): StoryEdits | undefined => {
		const edits = operations().story_edits;
		return edits && typeof edits === "object" ? edits : undefined;
	};
	const canEditStory = () =>
		can("update_story") ||
		(can("story_edits") &&
			["title", "priority", "problem", "criteria_add"].some(
				(field) => storyEdits()?.[field as keyof StoryEdits] === true,
			));
	// A dependency choice belongs to the story it was made for.
	createEffect(on(storyId, () => setDependencyId("")));

	function changeFilters(): void {
		void refresh();
	}

	const selectedPlan = () => plans().find((plan) => plan.id === planId());
	const selectedStory = () => stories().find((story) => story.id === storyId());
	const storyPlan = () => plans().find((plan) => plan.id === selectedStory()?.planId);
	const planTitleFor = (story: Story) => plans().find((plan) => plan.id === story.planId)?.title ?? story.planId;
	const fail = (cause: unknown): void => {
		const message = String(cause);
		appLogger.warn("store", "Stories: action failed", { error: message });
		const detail = cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : message;
		setError(
			detail.startsWith("project_unavailable:")
				? t("stories.error.projectUnavailable", "Cannot read {path}", { path: props.project })
				: detail,
		);
	};
	const ensureCapabilities = async (current: number, project: string): Promise<void> => {
		try {
			const capabilities = await invoke<unknown>("story_capabilities", { project });
			if (!isStoryCapabilities(capabilities)) throw new MissingStoriesBackendError();
			if (current !== request || project !== props.project) return;
			setOperations((capabilities as { operations: Record<string, boolean | StoryEdits> }).operations);
			setCapabilitiesReady(true);
		} catch (cause) {
			if (current === request && project === props.project) {
				setCapabilitiesReady(false);
				setBackendStatus((status) =>
					status ? { ...status, availability: "unavailable", error: String(cause) } : status,
				);
			}
			if (
				cause instanceof MissingStoriesBackendError ||
				(isTauri() && String(cause).includes("story_capabilities not found")) ||
				(cause instanceof HttpRpcError && cause.status === 404)
			) {
				throw new MissingStoriesBackendError();
			}
			throw cause;
		}
	};
	const call = async (action: Record<string, unknown>): Promise<Reply> => {
		const reply = await invoke<unknown>("story_action_command", { project: props.project, action });
		if (typeof reply === "string" && /^\s*<!doctype html|^\s*<html/i.test(reply)) {
			throw new Error(missingBackendMessage());
		}
		return reply as Reply;
	};

	function invalidateBackendContent(): void {
		setCapabilitiesReady(false);
		setPlans([]);
		setStories([]);
		setPlanId(null);
		setStoryId(null);
		setEditingPlan(null);
		setEditing(null);
		setNewPlan(false);
		setNewStory(false);
	}

	async function refresh(preferredPlan = planId(), preferredStory = storyId()): Promise<void> {
		const current = ++request;
		const project = props.project;
		setLoading(true);
		setError("");
		try {
			let status: Reply;
			try {
				status = await call({ action: "backend_status" });
			} catch (cause) {
				if (
					(cause instanceof HttpRpcError && cause.status === 404) ||
					String(cause).includes("story_capabilities not found")
				)
					throw new MissingStoriesBackendError();
				throw cause;
			}
			if (current !== request || project !== props.project) return;
			if (status?.type !== "backend_status") throw new MissingStoriesBackendError();
			if (status.type === "backend_status") {
				const previous = backendStatus();
				setBackendStatus(status.value);
				if (
					(previous &&
						(previous.backend !== status.value.backend ||
							previous.generation !== status.value.generation ||
							previous.version !== status.value.version)) ||
					status.value.availability !== "available"
				) {
					invalidateBackendContent();
					preferredPlan = null;
					preferredStory = null;
				}
				if (status.value.availability !== "available") throw new Error(status.value.error ?? status.value.reason);
			}
			await ensureCapabilities(current, project);
			if (current !== request || project !== props.project) return;
			const list = await call({ action: "list_plans", ...(archivedPlans() ? { archived: true } : {}) });
			if (list.type !== "plans") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
			let available: PlanSource[] | undefined;
			if (list.value.length === 0 && !archivedPlans()) {
				const sources = await call({ action: "list_plan_sources" });
				if (sources.type !== "plan_sources") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
				available = sources.value;
			}
			const nextPlan =
				preferredPlan === ALL_PLANS
					? ALL_PLANS
					: (list.value.find((plan) => plan.id === preferredPlan)?.id ?? list.value[0]?.id ?? null);
			let nextStories: Story[] = [];
			let nextState = "draft";
			let nextWontFixCount = 0;
			let nextAllCancelled = false;
			let nextVisibleIds: string[] | undefined;
			let nextColumns: StoryColumn[] = [];
			if (nextPlan === ALL_PLANS) {
				const view = await call({
					action: "project_story_view",
					archived_plans: archivedPlans(),
					filter: untrack(filter),
				});
				if (view.type !== "project_story_view")
					throw new Error(t("stories.error.invalidStory", "Invalid story response"));
				nextStories = view.value.stories;
				nextVisibleIds = view.value.visibleStoryIds;
				nextColumns = view.value.columns;
			} else if (nextPlan) {
				const selectedFilter = untrack(filter);
				const filtering =
					selectedFilter.visibility !== "all" ||
					selectedFilter.storyType !== null ||
					selectedFilter.untypedOnly ||
					selectedFilter.archived;
				const view = await call(
					filtering
						? { action: "filtered_plan_view", plan_id: nextPlan, filter: selectedFilter }
						: { action: "plan_view", plan_id: nextPlan },
				);
				if (view.type !== "plan_view") throw new Error(t("stories.error.invalidStory", "Invalid story response"));
				nextStories = view.value.stories;
				nextVisibleIds = view.value.visibleStoryIds;
				nextColumns = view.value.columns ?? [];
				nextState = view.value.state;
				nextWontFixCount = view.value.wontFixCount;
				nextAllCancelled = view.value.allCancelled;
			}
			const nextChoices = new Map<string, PhaseChoices>();
			// Four reads at a time; a large plan must not fan out unbounded requests.
			const actionable = nextStories.filter(
				(story) => !story.archived && !list.value.find((plan) => plan.id === story.planId)?.archived,
			);
			let index = 0;
			await Promise.all(
				Array.from({ length: Math.min(4, actionable.length) }, async () => {
					while (index < actionable.length && current === request) {
						const story = actionable[index++];
						try {
							const reply = await call({
								action: "available_transitions",
								story_id: story.id,
								expected_revision: story.revision,
							});
							if (reply.type !== "available_transitions") throw new Error("Invalid phase transitions response");
							nextChoices.set(story.id, reply.value);
						} catch (cause) {
							nextChoices.set(story.id, {
								revision: story.revision,
								moves: [],
								error:
									cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause),
							});
						}
					}
				}),
			);
			const confirmed = await call({ action: "backend_status" });
			if (current !== request || project !== props.project) return;
			if (confirmed.type !== "backend_status" || JSON.stringify(confirmed.value) !== JSON.stringify(status.value)) {
				batch(() => {
					setBackendStatus(confirmed.type === "backend_status" ? confirmed.value : null);
					invalidateBackendContent();
				});
				throw new Error(t("stories.backend.changed", "Story backend changed during refresh; refresh again."));
			}
			if (available) {
				setPlanSources(available);
				if (available.length > 0) setNewPlan(true);
			}
			setReadFailed(false);
			setPlans(list.value);
			setPlanId(nextPlan);
			setStories(nextStories);
			setPhaseChoices(nextChoices);
			setVisibleIds(nextVisibleIds);
			setColumns(nextColumns);
			setPlanState(nextState);
			setWontFixCount(nextWontFixCount);
			setAllCancelled(nextAllCancelled);
			const selectableIds = nextVisibleIds ?? nextStories.map((story) => story.id);
			setStoryId(
				preferredStory && selectableIds.includes(preferredStory) ? preferredStory : (selectableIds[0] ?? null),
			);
		} catch (cause) {
			if (current === request) {
				setReadFailed(true);
				fail(cause);
			}
		} finally {
			if (current === request) setLoading(false);
		}
	}

	async function mutate(
		action: Record<string, unknown>,
		preferredPlan = planId(),
		preferredStory = storyId(),
	): Promise<Reply | undefined> {
		setBusy(true);
		setError("");
		try {
			const result = await call(action);
			await refresh(preferredPlan, preferredStory);
			return result;
		} catch (cause) {
			fail(cause);
			return undefined;
		} finally {
			setBusy(false);
		}
	}

	async function savePlan(patch: PlanEditPatch): Promise<void> {
		const snapshot = editingPlan();
		if (!snapshot) return;
		setBusy(true);
		setPlanEditError("");
		try {
			const reply = await call({
				action: "update_plan",
				plan_id: snapshot.id,
				expected_revision: snapshot.revision,
				patch,
			});
			if (reply.type !== "plan") throw new Error("Invalid plan update response");
			if (editingPlan() === snapshot) setEditingPlan(null);
			await refresh();
		} catch (cause) {
			appLogger.warn("store", "Stories: plan update failed", { error: String(cause) });
			const message =
				cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause);
			if (editingPlan() === snapshot)
				setPlanEditError(
					message.includes("stale plan revision")
						? t(
								"stories.planEdit.conflict",
								"This plan changed after editing started. Your draft is kept. Cancel and reopen Edit plan to load the latest revision.",
							)
						: message,
				);
		} finally {
			setBusy(false);
		}
	}

	function cancelPlanEdit(): void {
		setEditingPlan(null);
		setPlanEditError("");
		void refresh();
	}

	async function saveStory(patch: StoryEditPatch): Promise<void> {
		const snapshot = editing();
		if (!snapshot) return;
		setBusy(true);
		setEditError("");
		try {
			const reply = await call({
				action: "update_story",
				story_id: snapshot.id,
				expected_revision: snapshot.revision,
				patch,
			});
			if (reply.type !== "story") throw new Error("Invalid story update response");
			if (editing() === snapshot) setEditing(null);
			await refresh();
		} catch (cause) {
			appLogger.warn("store", "Stories: amendment failed", { error: String(cause) });
			const message =
				cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause);
			if (editing() === snapshot)
				setEditError(
					message.includes("stale story revision")
						? t(
								"stories.edit.conflict",
								"This story changed after editing started. Your draft is kept. Cancel and reopen Edit to load the latest revision.",
							)
						: message,
				);
		} finally {
			setBusy(false);
		}
	}

	function cancelEdit(): void {
		setEditing(null);
		setEditError("");
		void refresh();
	}

	async function createPlan(event: Event): Promise<void> {
		event.preventDefault();
		const source = planSource().trim();
		const result = await addPlanSource(source, /^https?:\/\//i.test(source) ? planTitle().trim() : undefined);
		if (result) {
			setNewPlan(false);
			setPlanTitle("");
			setPlanSource("");
		}
	}
	async function loadPlanSources(): Promise<void> {
		setSourcesLoading(true);
		setError("");
		try {
			const reply = await call({ action: "list_plan_sources" });
			if (reply.type !== "plan_sources") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
			setPlanSources(reply.value);
		} catch (cause) {
			fail(cause);
		} finally {
			setSourcesLoading(false);
		}
	}
	async function addPlanSource(source: string, linkTitle?: string): Promise<boolean> {
		setBusy(true);
		setError("");
		try {
			const reply = await call(
				linkTitle === undefined
					? { action: "add_plan_source", source }
					: { action: "create_plan", title: linkTitle, source },
			);
			if (reply.type !== "plan") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
			await refresh(reply.value.id, null);
			setPlanSources((sources) => sources.filter((candidate) => candidate.source !== source));
			setNewPlan(false);
			return true;
		} catch (cause) {
			fail(cause);
			return false;
		} finally {
			setBusy(false);
		}
	}

	async function createStory(event: Event): Promise<void> {
		event.preventDefault();
		if (filling() || busy()) return;
		const currentPlan = planId();
		if (!currentPlan) return;
		const criteria = criteriaText()
			.split(/\r?\n/)
			.map((line) => line.trim())
			.filter(Boolean);
		const fileScope = scopeText()
			.split(/\r?\n/)
			.map((line) => line.trim())
			.filter(Boolean);
		if (criteria.length === 0) {
			setError(t("stories.error.criteriaRequired", "Add at least one acceptance criterion."));
			return;
		}
		setBusy(true);
		setError("");
		try {
			const reply = await call({
				action: "create_story",
				...(storyProblem().trim() ? { problem: storyProblem().trim() } : {}),
				input: {
					planId: currentPlan,
					title: storyTitle().trim(),
					criteria,
					priority: priority(),
					...(newStoryType() === "untyped" ? {} : { storyType: newStoryType() }),
					origin: originType() === "plan_step" ? { type: "plan_step", step: planStep().trim() } : { type: "native" },
					fileScope,
				},
			});
			if (reply.type !== "story") throw new Error(t("stories.error.invalidStory", "Invalid story response"));
			await refresh(currentPlan, reply.value.id);
			setNewStory(false);
			setStoryTitle("");
			setStoryRequest("");
			setStoryProblem("");
			setFillNotice("");
			setFillError("");
			setCriteriaText("");
			setScopeText("");
			setNewStoryType("untyped");
			setOriginType("native");
			setPlanStep("");
		} catch (cause) {
			fail(cause);
		} finally {
			setBusy(false);
		}
	}

	function moveStory(story: Story, command: string): void {
		if (busy() || loading()) return;
		const choices = phaseChoices().get(story.id);
		if (choices?.revision !== story.revision || !choices.moves.some((move) => move.command === command)) return;
		submitTransition(story, command);
	}
	function submitTransition(story: Story, command: string | Record<string, number>): void {
		if (busy() || loading()) return;
		void mutate({ action: "transition", story_id: story.id, expected_revision: story.revision, command });
	}

	const legalMove = (story: Story, status: StoryStatus) => {
		const choices = phaseChoices().get(story.id);
		return choices?.revision === story.revision ? choices.moves.find((move) => move.status === status) : undefined;
	};
	const columnAt = (x: number, y: number): StoryStatus | null => {
		for (const column of boardRoot?.querySelectorAll<HTMLElement>("[data-story-column]") ?? []) {
			const rect = column.getBoundingClientRect();
			if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom)
				return column.dataset.storyColumn as StoryStatus;
		}
		return null;
	};
	function startStoryDrag(event: PointerEvent, story: Story): void {
		if (
			busy() ||
			loading() ||
			editing() ||
			draggedStory() ||
			story.archived ||
			!phaseChoices().get(story.id)?.moves.length
		)
			return;
		const source = event.currentTarget as HTMLElement;
		cancelStoryDrag?.();
		cancelStoryDrag = initMouseDrag(event, source, {
			onStart: () => {
				suppressCardClick = true;
				setDraggedStory(story);
			},
			onMove: (x, y) => {
				const status = columnAt(x, y);
				setDropTarget(status && legalMove(story, status) ? status : null);
			},
			onDrop: (x, y) => {
				const active = draggedStory();
				setDraggedStory(null);
				setDropTarget(null);
				if (!active) return;
				const status = columnAt(x, y);
				if (!status || status === active.status) return;
				const move = legalMove(active, status);
				if (move) moveStory(active, move.command);
				else setError(t("stories.phase.illegal", "No legal transition to {phase}.", { phase: statusLabel(status) }));
			},
			onCancel: () => {
				setDraggedStory(null);
				setDropTarget(null);
			},
		});
	}

	function transition(command: string | Record<string, number>): void {
		const story = selectedStory();
		if (!story) return;
		submitTransition(story, command);
	}

	function addDependency(): void {
		const story = selectedStory();
		const dep = dependencyId();
		if (!story || !dep) return;
		void mutate({
			action: "add_dependency",
			story_id: story.id,
			dependency_id: dep,
			expected_revision: story.revision,
		});
		setDependencyId("");
	}

	function removeDependency(dependencyId: string): void {
		const story = selectedStory();
		if (story?.status !== "backlog") return;
		void mutate({
			action: "remove_dependency",
			story_id: story.id,
			dependency_id: dependencyId,
			expected_revision: story.revision,
		});
	}

	/** Land on a waiting item's source: its plan and story. */
	async function reveal(target: Pick<StoriesTarget, "planId" | "storyId">): Promise<void> {
		await refresh(target.planId ?? planId(), target.storyId ?? storyId());
	}

	createEffect(
		on(
			() => [props.project, repositoriesStore.getRevision(props.project)] as const,
			([project], previous) => {
				if (previous && previous[0] === project) {
					void refresh();
					return;
				}
				++request;
				setBackendStatus(null);
				setCapabilitiesReady(false);
				setPlans([]);
				setStories([]);
				void refresh(null, null);
			},
			{ defer: true },
		),
	);

	async function approveConnector(): Promise<void> {
		const installation = backendStatus()?.installation;
		const expected_identity = backendStatus()?.generation;
		if (!installation || !expected_identity) return;
		setBusy(true);
		try {
			await call({ action: "approve_connector", installation, expected_identity });
			await refresh(null, null);
		} catch (cause) {
			fail(cause);
		} finally {
			setBusy(false);
		}
	}

	onMount(() => {
		closeButton?.focus();
		const target = storiesUi.takeTarget(props.project);
		void (target ? reveal(target) : refresh());
	});
	createEffect(
		on(
			storiesUi.target,
			() => {
				const target = storiesUi.takeTarget(props.project);
				if (target) void reveal(target);
			},
			{ defer: true },
		),
	);

	return (
		<div
			class={mode() === "detached" ? s.detachedHost : d.overlay}
			onClick={() => mode() === "inline" && props.onClose()}
		>
			<div
				class={s.dialog}
				classList={{ [s.detached]: mode() === "detached" }}
				role="dialog"
				aria-modal={mode() === "inline" ? "true" : undefined}
				aria-label={t("stories.title", "Plans and Stories")}
				onClick={(event) => event.stopPropagation()}
			>
				<header class={s.header}>
					<div>
						<h2>{t("stories.title", "Plans and Stories")}</h2>
						<Show
							when={backendStatus()}
							fallback={<span>{t("stories.backend.checking", "Checking story backend…")}</span>}
						>
							{(status) => (
								<div aria-label={t("stories.backend.label", "Story backend")}>
									<span>
										{status().label} · {status().provenance} · {status().availability}
									</span>
									<details>
										<summary>{status().reason}</summary>
										<p>
											{status().backend} {status().version}
										</p>
										<p>{status().installation}</p>
										<p>{status().generation}</p>
										<Show when={status().error}>
											<p>{status().error}</p>
										</Show>
										<Button
											size="compact"
											type="button"
											disabled={busy()}
											onClick={() => void mutate({ action: "set_backend", backend: "native" }, null, null)}
										>
											{t("stories.backend.native", "Use TUIC native")}
										</Button>
										<Button
											size="compact"
											type="button"
											disabled={busy()}
											onClick={() => void mutate({ action: "set_backend", backend: "automatic" }, null, null)}
										>
											{t("stories.backend.automatic", "Use automatic selection")}
										</Button>
									</details>
									<Show when={status().availability === "approval_required" && status().installation}>
										<p>
											{t(
												"stories.backend.approvalHint",
												"Approve this entry/version for applicable Wiz projects. Dependencies are not fingerprinted; a changed entry/version needs new approval.",
											)}
										</p>
										<p>{status().installation}</p>
										<p>{status().generation}</p>
										<Button size="compact" type="button" disabled={busy()} onClick={() => void approveConnector()}>
											{t("stories.backend.approve", "Approve installed Wiz connector")}
										</Button>
									</Show>
								</div>
							)}
						</Show>
						<span class={s.project} title={props.project}>
							{props.project}
						</span>
					</div>
					<div class={s.headerActions}>
						{/* Definitions and the Designer live in the Workflows space (Boss Q6=A); it opens on top, so this selection stays. */}
						<Button size="compact" type="button" onClick={() => workflowsUi.open(props.project)}>
							Workflows
						</Button>
					</div>
					<div class={s.windowControls}>
						<PanelWindowControls
							panelId="stories"
							mode={mode()}
							onInlineClose={props.onClose}
							closeLabel={t("stories.close", "Close Plans and Stories")}
							closeButtonRef={(button) => {
								closeButton = button;
							}}
						/>
					</div>
				</header>
				<WaitingOnYou
					project={props.project}
					onSelect={(item) => void reveal({ ...item, storyId: item.story?.id ?? null })}
				/>
				<Show when={error()}>
					<div class={s.error} role="alert">
						{error()}{" "}
						<Button type="button" onClick={() => void refresh()}>
							{t("stories.retry", "Retry")}
						</Button>
					</div>
				</Show>
				<Show when={loading()}>
					<div class={s.message} role="status">
						{t("stories.loading", "Loading plans and stories…")}
					</div>
				</Show>
				<Show when={editingPlan()} keyed>
					{(plan) => (
						<section class={s.editorContent} aria-label={t("stories.planEdit.open", "Edit plan")}>
							<PlanEditForm
								plan={plan}
								sources={planSources()}
								sourcesLoading={sourcesLoading()}
								busy={busy()}
								error={planEditError()}
								onRefreshSources={() => void loadPlanSources()}
								onOpenSource={(source) => {
									if (/^https?:\/\//i.test(source)) handleOpenUrl(source);
									else {
										openFileAction(source, props.project);
										props.onClose();
									}
								}}
								onSave={savePlan}
								onCancel={cancelPlanEdit}
							/>
						</section>
					)}
				</Show>
				<Show when={newStory() && !editingPlan()}>
					<section class={s.editorContent} aria-label={t("stories.newStory", "New story")}>
						<div class={s.editorHeading}>
							<h3>{t("stories.newStory", "New story")}</h3>
						</div>
						<Show when={newStory() && planId() && !selectedPlan()?.archived}>
							<form class={s.form} onSubmit={(event) => void createStory(event)}>
								<label>
									{t("stories.ai.request", "Request")}
									<Textarea
										value={storyRequest()}
										maxlength={20000}
										disabled={filling()}
										onInput={(event) => setStoryRequest(event.currentTarget.value)}
									/>
								</label>
								<div class={s.headerActions}>
									<Button disabled={filling() || busy() || !storyRequest().trim()} onClick={() => void fillStory()}>
										{t("stories.ai.fill", "Fill with AI")}
									</Button>
									<Show when={filling()}>
										<Button disabled={cancellingFill()} onClick={() => void cancelFill()}>
											{t("stories.ai.cancel", "Cancel fill")}
										</Button>
									</Show>
								</div>
								<Show when={filling()}>
									<p role="status" class={s.muted}>
										{cancellingFill()
											? t("stories.ai.cancelling", "Cancelling fill…")
											: t("stories.ai.filling", "Filling story fields…")}
									</p>
								</Show>
								<Show when={fillNotice()}>
									<p role="status" class={s.muted}>
										{fillNotice()}
									</p>
								</Show>
								<Show when={fillError()}>
									<p role="alert" class={s.error}>
										{fillError()}
									</p>
								</Show>
								<label>
									{t("stories.titleLabel", "Title")}
									<Input
										disabled={filling()}
										required
										maxlength="200"
										value={storyTitle()}
										onInput={(event) => setStoryTitle(event.currentTarget.value)}
									/>
								</label>
								<label>
									{t("stories.edit.problem", "Problem (optional)")}
									<Textarea
										maxlength={20000}
										disabled={filling()}
										value={storyProblem()}
										onInput={(event) => setStoryProblem(event.currentTarget.value)}
									/>
								</label>
								<label>
									{t("stories.criteriaInput", "Acceptance criteria, one per line")}
									<Textarea
										disabled={filling()}
										required
										value={criteriaText()}
										onInput={(event) => setCriteriaText(event.currentTarget.value)}
									/>
								</label>
								<label>
									{t("stories.scopeInput", "File scope, one relative path per line")}
									<Textarea
										disabled={filling()}
										value={scopeText()}
										onInput={(event) => setScopeText(event.currentTarget.value)}
									/>
								</label>
								<label>
									{t("stories.typeLabel", "Story type")}
									<Select
										disabled={filling()}
										value={newStoryType()}
										onChange={(event) => setNewStoryType(event.currentTarget.value as StoryType | "untyped")}
									>
										<option value="untyped">{storyTypeLabel(undefined)}</option>
										<For each={STORY_TYPES}>{(kind) => <option value={kind}>{storyTypeLabel(kind)}</option>}</For>
									</Select>
								</label>
								<label>
									{t("stories.metadata.origin", "Origin")}
									<Select
										disabled={filling()}
										value={originType()}
										onChange={(event) => setOriginType(event.currentTarget.value as "native" | "plan_step")}
									>
										<option value="native">{t("stories.metadata.native", "Native")}</option>
										<option value="plan_step">{t("stories.metadata.planStep", "Plan step")}</option>
									</Select>
								</label>
								<Show when={originType() === "plan_step"}>
									<label>
										{t("stories.metadata.planStep", "Plan step")}
										<Input
											disabled={filling()}
											required
											maxlength="200"
											value={planStep()}
											onInput={(event) => setPlanStep(event.currentTarget.value)}
										/>
									</label>
								</Show>
								<label>
									{t("stories.priority", "Priority")}
									<Select
										disabled={filling()}
										value={priority()}
										onChange={(event) => setPriority(Number(event.currentTarget.value))}
									>
										<option value="1">P1</option>
										<option value="2">P2</option>
										<option value="3">P3</option>
									</Select>
								</label>
								<div class={s.inline}>
									<Button variant="primary" type="submit" disabled={busy() || filling()}>
										{t("stories.createStory", "Create story")}
									</Button>
									<Button
										disabled={busy()}
										onClick={() => {
											void cancelFill();
											setNewStory(false);
										}}
									>
										{t("stories.edit.cancel", "Cancel")}
									</Button>
								</div>
							</form>
						</Show>
					</section>
				</Show>
				<Show when={!editingPlan() && !newStory() && !readFailed()}>
					<div class={s.content} classList={{ [s.kanbanContent]: storiesUi.view() === "kanban" }}>
						<aside class={s.planColumn} aria-label={t("stories.plans", "Plans")}>
							<div class={s.columnHeader}>
								<h3>{t("stories.plans", "Plans")}</h3>
								<Button
									type="button"
									disabled={!can("create_plan")}
									onClick={() => {
										if (!newPlan()) void loadPlanSources();
										setNewPlan(!newPlan());
									}}
								>
									{t("stories.newPlan", "New plan")}
								</Button>
							</div>
							<div class={s.filters}>
								<label class={s.archiveToggle}>
									<input
										type="checkbox"
										checked={archivedPlans()}
										disabled={busy()}
										onChange={(event) => {
											setArchivedPlans(event.currentTarget.checked);
											setNewPlan(false);
											void refresh(planId() === ALL_PLANS ? ALL_PLANS : null, null);
										}}
									/>
									{t("stories.archive.plans", "Archived plans")}
								</label>
							</div>
							<Show when={selectedPlan()}>
								<div class={s.planActions} role="group" aria-label={t("stories.planEdit.actions", "Plan actions")}>
									<Button
										size="compact"
										disabled={busy() || !can("update_plan")}
										onClick={() => {
											const plan = selectedPlan();
											if (plan) {
												setEditingPlan({ ...plan });
												setPlanEditError("");
												void loadPlanSources();
											}
										}}
									>
										{t("stories.planEdit.open", "Edit plan")}
									</Button>
									<Show when={selectedPlan()}>
										{(plan) => (
											<Button
												size="compact"
												type="button"
												disabled={busy() || !can("archive_plan")}
												onClick={() =>
													void mutate({
														action: plan().archived ? "unarchive_plan" : "archive_plan",
														plan_id: plan().id,
														expected_revision: plan().revision,
													})
												}
											>
												{plan().archived
													? t("stories.archive.restorePlan", "Restore plan")
													: t("stories.archive.archivePlan", "Archive plan")}
											</Button>
										)}
									</Show>
								</div>
							</Show>
							<Show when={selectedPlan() && wontFixCount() > 0}>
								<p class={s.planNotice}>
									{allCancelled()
										? t("stories.allCancelled", "All cancelled")
										: t("stories.wontFixCount", "{count} won't fix", { count: String(wontFixCount()) })}
								</p>
							</Show>
							<Show when={newPlan()}>
								<div class={s.form}>
									<div class={s.sourceHeader}>
										<span>{t("stories.existingPlans", "Plans in this project")}</span>
										<Button
											size="compact"
											type="button"
											disabled={sourcesLoading()}
											onClick={() => void loadPlanSources()}
										>
											{t("stories.refreshPlans", "Refresh")}
										</Button>
									</div>
									<Show when={sourcesLoading()}>
										<span>{t("stories.loading", "Loading plans and stories…")}</span>
									</Show>
									<For each={planSources().filter((source) => !plans().some((plan) => plan.source === source.source))}>
										{(candidate) => (
											<button
												data-control-layout="row"
												class={s.item}
												type="button"
												disabled={busy()}
												onClick={() => void addPlanSource(candidate.source)}
											>
												{candidate.title}
												<small>{candidate.source}</small>
											</button>
										)}
									</For>
									<details>
										<summary>{t("stories.addFromPath", "Add from path or link")}</summary>
										<form class={s.manualForm} onSubmit={(event) => void createPlan(event)}>
											<label>
												{t("stories.planSource", "Plan document or link")}
												<Input
													required
													value={planSource()}
													onInput={(event) => setPlanSource(event.currentTarget.value)}
												/>
											</label>
											<Show when={/^https?:\/\//i.test(planSource())}>
												<label>
													{t("stories.titleLabel", "Title")}
													<Input
														required
														maxlength="200"
														value={planTitle()}
														onInput={(event) => setPlanTitle(event.currentTarget.value)}
													/>
												</label>
											</Show>
											<Button variant="primary" type="submit" disabled={busy()}>
												{t("stories.createPlan", "Create plan")}
											</Button>
										</form>
									</details>
								</div>
							</Show>
							<Show when={!loading() && plans().length === 0}>
								<p class={s.empty}>{t("stories.emptyPlans", "Create a plan to begin.")}</p>
							</Show>
							<nav class={s.items}>
								<button
									data-control-layout="row"
									type="button"
									class={s.item}
									aria-current={planId() === ALL_PLANS ? "true" : undefined}
									onClick={() => void refresh(ALL_PLANS, null)}
								>
									{t("stories.allPlans", "All plans")}
								</button>
								<For each={plans()}>
									{(plan) => (
										<button
											data-control-layout="row"
											type="button"
											class={s.item}
											aria-current={planId() === plan.id ? "true" : undefined}
											onClick={() => void refresh(plan.id, null)}
										>
											{plan.title}
										</button>
									)}
								</For>
							</nav>
						</aside>
						<section class={s.storyColumn} aria-label={t("stories.stories", "Stories")}>
							<div class={s.columnHeader}>
								<h3>{t("stories.stories", "Stories")}</h3>
								<Button
									type="button"
									disabled={!selectedPlan() || selectedPlan()?.archived || !can("create_story")}
									onClick={() => setNewStory(!newStory())}
								>
									{t("stories.newStory", "New story")}
								</Button>
							</div>
							<div class={s.filters}>
								<label class={s.archiveToggle}>
									<input
										type="checkbox"
										checked={archivedStories()}
										disabled={busy()}
										onChange={(event) => {
											setArchivedStories(event.currentTarget.checked);
											void refresh(planId(), null);
										}}
									/>
									{t("stories.archive.stories", "Archived stories")}
								</label>
								<label>
									{t("stories.filter.status", "Status")}
									<Select
										value={storiesUi.visibility()}
										onChange={(event) => {
											storiesUi.setVisibility(event.currentTarget.value as StoryVisibility);
											changeFilters();
										}}
									>
										<option value="all">{t("stories.filter.allStatuses", "All statuses")}</option>
										<option value="open">{t("stories.filter.open", "Open only")}</option>
										<option value="closed">{t("stories.filter.closed", "Closed only")}</option>
										<option value="qa">{t("stories.filter.qa", "QA only")}</option>
									</Select>
								</label>
								<label>
									{t("stories.typeLabel", "Story type")}
									<Select
										value={storiesUi.storyType()}
										onChange={(event) => {
											storiesUi.setStoryType(event.currentTarget.value as StoryType | "all" | "untyped");
											changeFilters();
										}}
									>
										<option value="all">{t("stories.filter.allTypes", "All types")}</option>
										<option value="untyped">{storyTypeLabel(undefined)}</option>
										<For each={STORY_TYPES}>{(kind) => <option value={kind}>{storyTypeLabel(kind)}</option>}</For>
									</Select>
								</label>
								<SegmentedControl
									class={s.viewSwitch}
									aria-label={t("stories.view.label", "Story view")}
									value={storiesUi.view()}
									onChange={storiesUi.setView}
									options={[
										{ id: "list", label: t("stories.view.list", "List") },
										{ id: "kanban", label: t("stories.view.kanban", "Kanban") },
									]}
								/>
							</div>

							<Show when={!loading() && planId() && listedStories().length === 0}>
								<p class={s.empty}>
									{stories().length || planId() === ALL_PLANS
										? t("stories.filter.empty", "No stories match these filters.")
										: t("stories.emptyStories", "This plan has no stories.")}
								</p>
							</Show>
							{/* DEFERRED (2026-10-23) — core and the independently released Wiz plugin keep
                                separate boards; extract a shared package when a third consumer needs one. */}
							<Show
								when={storiesUi.view() === "kanban"}
								fallback={
									<nav class={s.items}>
										<For each={listedStories()}>
											{(story) => (
												<div class={s.storyListRow}>
													<button
														data-control-layout="row"
														type="button"
														class={s.item}
														aria-current={storyId() === story.id ? "true" : undefined}
														onClick={() => setStoryId(story.id)}
													>
														<span>{story.title}</span>
														<small>{planTitleFor(story)}</small>
														<small>
															{statusLabel(story.status)}
															{story.completedOutside
																? ` · ${t("stories.completedOutside", "completed outside TUIC")}`
																: ""}{" "}
															· {storyTypeLabel(story.storyType)}
														</small>
													</button>
													<PhaseSelector
														story={story}
														choices={phaseChoices().get(story.id)}
														disabled={busy() || loading() || !!editing() || !can("transitions")}
														label={t("stories.phase.forStory", "Phase for {title}", { title: story.title })}
														onMove={moveStory}
													/>
												</div>
											)}
										</For>
									</nav>
								}
							>
								<div ref={boardRoot} class={s.board} role="region" aria-label={t("stories.view.kanban", "Kanban")}>
									<For each={columns()}>
										{(column) => (
											<section
												class={s.boardColumn}
												aria-label={statusLabel(column.status)}
												data-story-column={column.status}
												data-drop-allowed={
													draggedStory() && legalMove(draggedStory()!, column.status) ? "true" : undefined
												}
												data-drop-target={dropTarget() === column.status ? "true" : undefined}
											>
												<header class={s.boardColumnHeader} data-status={column.status}>
													<span>{statusLabel(column.status)}</span>
													<span>{column.storyIds.length}</span>
												</header>
												<div class={s.boardCards}>
													<Show
														when={column.storyIds.length}
														fallback={<p class={s.boardEmpty}>{t("stories.board.empty", "No stories")}</p>}
													>
														<For each={column.storyIds}>
															{(id) => (
																<Show when={storiesById().get(id)}>
																	{(story) => (
																		<button
																			data-control-layout="card"
																			type="button"
																			class={s.boardCard}
																			aria-current={storyId() === id ? "true" : undefined}
																			onPointerDown={(event) => {
																				suppressCardClick = false;
																				startStoryDrag(event, story());
																			}}
																			onClick={(event) => {
																				if (suppressCardClick) {
																					suppressCardClick = false;
																					event.preventDefault();
																					return;
																				}
																				setStoryId(id);
																			}}
																		>
																			<small class={s.boardCardId}>{story().id}</small>
																			<span>{story().title}</span>
																			<small>{planTitleFor(story())}</small>
																			<small>
																				{storyTypeLabel(story().storyType)} · P{story().priority}
																			</small>
																		</button>
																	)}
																</Show>
															)}
														</For>
													</Show>
												</div>
											</section>
										)}
									</For>
								</div>
							</Show>
						</section>
						<section class={s.detail} aria-label={t("stories.details", "Story details")}>
							<Show when={selectedPlan()}>
								<p class={s.planState}>
									{t("stories.state", "State")}: {planStateLabel(planState())}
								</p>
							</Show>
							<Show
								when={selectedStory()}
								fallback={
									<Show
										when={selectedPlan()}
										fallback={<p class={s.empty}>{t("stories.selectPlan", "Select or create a plan.")}</p>}
									>
										{(plan) => (
											<div class={s.planDetail}>
												<h3>{plan().title}</h3>
												<p>
													{t("stories.source", "Source")}: {plan().source}
												</p>
											</div>
										)}
									</Show>
								}
							>
								{(story) => (
									<>
										<div class={s.detailHeader}>
											<div>
												<span class={s.eyebrow}>
													{t("stories.story", "Story")} · P{story().priority} · {storyTypeLabel(story().storyType)}
												</span>
												<h3>{story().title}</h3>
											</div>
											<div class={s.storyHeaderActions} data-story-header-actions>
												<Show
													when={
														!editing() &&
														!story().archived &&
														!storyPlan()?.archived &&
														["backlog", "ready"].includes(story().status) &&
														!story().claimSession
													}
												>
													<Button
														variant="primary"
														type="button"
														disabled={busy() || !canEditStory()}
														onClick={() => {
															setEditing(structuredClone(story()));
															setEditError("");
														}}
													>
														{t("stories.edit.open", "Edit")}
													</Button>
												</Show>
												<Show when={!editing()}>
													<PhaseSelector
														story={story()}
														choices={phaseChoices().get(story().id)}
														disabled={busy() || loading() || !can("transitions")}
														label={t("stories.phase.label", "Phase")}
														onMove={moveStory}
													/>
												</Show>
											</div>
										</div>
										<StoryMetadata project={props.project} story={story()} />
										<Show when={!editing() && !story().archived}>
											<StoryDelivery project={props.project} storyId={story().id} />
										</Show>
										<Show when={editing()} keyed>
											{(snapshot) => (
												<StoryEditForm
													story={snapshot}
													storyEdits={storyEdits()}
													candidates={stories().filter(
														(candidate) => candidate.planId === snapshot.planId && candidate.id !== snapshot.id,
													)}
													busy={busy()}
													error={editError()}
													onSave={saveStory}
													onCancel={cancelEdit}
												/>
											)}
										</Show>
										<Show when={!editing()}>
											<section>
												<h4>{t("stories.criteria", "Acceptance criteria")}</h4>
												<ul class={s.criteria}>
													<For each={story().criteria}>
														{(criterion, index) => (
															<li>
																<label>
																	<input
																		type="checkbox"
																		checked={story().checked[index()]}
																		disabled={
																			story().archived ||
																			storyPlan()?.archived ||
																			!["in_progress", "qa"].includes(story().status) ||
																			busy()
																		}
																		onChange={() =>
																			transition({
																				[story().checked[index()] ? "uncheck_criterion" : "check_criterion"]: index(),
																			})
																		}
																	/>
																	<span>{criterion}</span>
																</label>
															</li>
														)}
													</For>
												</ul>
											</section>
											<section>
												<h4>{t("stories.dependencies", "Dependencies")}</h4>
												<Show
													when={story().dependencies.length}
													fallback={<p class={s.muted}>{t("stories.none", "None")}</p>}
												>
													<ul>
														<For each={story().dependencies}>
															{(id) => {
																const dependency = stories().find((item) => item.id === id);
																return (
																	<li class={s.dependencyRow}>
																		<span>
																			{dependency?.title ?? id} ·{" "}
																			{dependency ? statusLabel(dependency.status) : t("stories.unknown", "Unknown")}
																			{dependency?.abandoned ? ` · ${t("stories.abandoned", "abandoned")}` : ""}
																		</span>
																		<Show
																			when={
																				!story().archived &&
																				!storyPlan()?.archived &&
																				story().status === "backlog" &&
																				dependency?.status === "wont_fix"
																			}
																		>
																			<Button
																				type="button"
																				disabled={busy() || !can("dependencies")}
																				aria-label={t("stories.removeNamedDependency", "Remove {title}", {
																					title: dependency?.title ?? id,
																				})}
																				onClick={() => removeDependency(id)}
																			>
																				{t("stories.remove", "Remove")}
																			</Button>
																		</Show>
																	</li>
																);
															}}
														</For>
													</ul>
												</Show>
												<Show
													when={
														!story().archived &&
														!storyPlan()?.archived &&
														(story().status === "ready" || story().status === "backlog")
													}
												>
													<p id={dependencyHintId} hidden>
														{dependencyReason()}
													</p>
													<div class={s.inline}>
														<Select
															aria-label={t("stories.addDependency", "Add dependency")}
															value={dependencyId()}
															onChange={(event) => setDependencyId(event.currentTarget.value)}
														>
															<option value="">{t("stories.selectStory", "Select story")}</option>
															<For
																each={stories().filter(
																	(candidate) =>
																		candidate.planId === story().planId &&
																		candidate.id !== story().id &&
																		!story().dependencies.includes(candidate.id),
																)}
															>
																{(candidate) => <option value={candidate.id}>{candidate.title}</option>}
															</For>
														</Select>
														<Button
															type="button"
															disabled={!!dependencyReason() || !can("dependencies")}
															title={dependencyReason()}
															aria-describedby={dependencyHintId}
															onClick={addDependency}
														>
															{t("stories.addDependency", "Add dependency")}
														</Button>
													</div>
												</Show>
											</section>
											<section>
												<h4>{t("stories.fileScope", "File scope")}</h4>
												<Show
													when={story().fileScope.length}
													fallback={<p class={s.muted}>{t("stories.unspecified", "Unspecified")}</p>}
												>
													<ul>
														<For each={story().fileScope}>{(path) => <li class={s.path}>{path}</li>}</For>
													</ul>
												</Show>
											</section>
											<div class={s.actions}>
												<Show when={!story().archived && !storyPlan()?.archived && can("transitions")}>
													<Show when={story().status === "ready"}>
														<Button
															variant="primary"
															type="button"
															disabled={busy()}
															onClick={() => transition("start_manual")}
														>
															{t("stories.startWork", "Start work")}
														</Button>
													</Show>
													<Show when={story().status === "in_progress"}>
														<Button
															variant="primary"
															type="button"
															disabled={busy()}
															onClick={() => transition("submit_review")}
														>
															{t("stories.submitReview", "Submit for review")}
														</Button>
													</Show>
													<Show when={story().status === "review"}>
														<Button
															variant="primary"
															type="button"
															disabled={busy()}
															onClick={() => transition("approve")}
														>
															{t("stories.approve", "Approve")}
														</Button>
														<Button type="button" disabled={busy()} onClick={() => transition("reject_review")}>
															{t("stories.requestChanges", "Request changes")}
														</Button>
													</Show>
													<Show when={["in_progress", "review"].includes(story().status)}>
														<Button type="button" disabled={busy()} onClick={() => transition("submit_qa")}>
															{t("stories.submitQa", "Submit for QA")}
														</Button>
													</Show>
													<Show when={story().status === "qa"}>
														<Button
															variant="primary"
															type="button"
															disabled={busy()}
															onClick={() => transition("approve")}
														>
															{t("stories.qaPassed", "QA passed")}
														</Button>
														<Button type="button" disabled={busy()} onClick={() => transition("reject_qa")}>
															{t("stories.qaFailed", "QA failed")}
														</Button>
													</Show>
													<Show when={["ready", "in_progress", "review", "qa"].includes(story().status)}>
														<Button type="button" disabled={busy()} onClick={() => transition("block")}>
															{t("stories.block", "Block")}
														</Button>
													</Show>
													<Show when={story().status === "blocked"}>
														<Button
															variant="primary"
															type="button"
															disabled={busy()}
															onClick={() => transition("unblock")}
														>
															{t("stories.unblock", "Unblock")}
														</Button>
													</Show>
													<Show when={story().status !== "done" && story().status !== "wont_fix"}>
														<Button
															variant="danger"
															type="button"
															disabled={busy()}
															onClick={() => transition("wont_fix")}
														>
															{t("stories.status.wontFix", "Won't fix")}
														</Button>
													</Show>
												</Show>
												<Button
													type="button"
													disabled={busy() || !!story().claimSession || !can("archive_story")}
													onClick={() =>
														void mutate({
															action: story().archived ? "unarchive_story" : "archive_story",
															story_id: story().id,
															expected_revision: story().revision,
														})
													}
												>
													{story().archived
														? t("stories.archive.restoreStory", "Restore story")
														: t("stories.archive.archiveStory", "Archive story")}
												</Button>
											</div>
										</Show>
										{/* Notes load independently; never suspend the host's whole-dialog lazy boundary. */}
										<Suspense
											fallback={
												<p class={s.muted} role="status">
													{t("stories.notes.loading", "Loading notes…")}
												</p>
											}
										>
											<Show when={`${props.project}:${story().id}`} keyed>
												{(_storyKey) => (
													<StoryNotes
														project={props.project}
														storyId={story().id}
														revision={story().revision}
														busy={busy() || !can("notes")}
													/>
												)}
											</Show>
										</Suspense>
									</>
								)}
							</Show>
						</section>
					</div>
				</Show>
			</div>
		</div>
	);
};
