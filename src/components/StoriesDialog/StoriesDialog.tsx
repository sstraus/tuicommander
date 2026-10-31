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
import { storiesUi } from "../../stores/storiesUi";
import { workflowRunSignals } from "../../stores/workflowRunSignals";
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
import { WorkflowDesigner } from "../WorkflowDesigner/WorkflowDesigner";
import { type EditablePlan, PlanEditForm, type PlanEditPatch } from "./PlanEditForm";
import { RunIncidents } from "./RunIncidents";
import s from "./StoriesDialog.module.css";
import { type EditableStory, StoryEditForm, type StoryEditPatch } from "./StoryEditForm";
import { StoryMetadata } from "./StoryMetadata";
import { StoryNotes } from "./StoryNotes";

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

const ALL_PLANS = "all-plans";

type Reply =
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

interface RunGraph {
	id: string;
	targetId: string;
	definition: {
		id: string;
		revision: number;
		graph: {
			nodes: { id: string; kind: { type: string } }[];
			edges: { from: string; to: string; outcome?: string | null }[];
		};
	};
	activations: { id: string; nodeId: string; state: string; edgeIndex: number | null }[];
	decisions: {
		activationId: string;
		edgeIndex: number;
		evidence: { actor: string; reason: string; references: string[] };
	}[];
	loops: { nodeId: string; repeats: number }[];
	pauses: { activationId: string; resumeTo: string; evidence: { reason: string }; resolution: string | null }[];
	completed: boolean;
}
interface RunQuestion {
	id: string;
	revision: number;
	attemptId: string;
	subjectDigest: string;
	state: string;
	schema: { type: string; options?: string[] };
	request: { question: string; options: string[] };
}
interface RunSnapshot {
	questions?: RunQuestion[];
	id: string;
	planId: string;
	status: string;
	pauseCause?: string;
	sequence: number;
	startedMs: number;
	rootTarget?: { type: string; id: string } | null;
	graphExecutions?: RunGraph[];
	stories: { storyId: string; accepted: boolean }[];
	attempts: {
		id: string;
		storyId: string;
		nodeId: string;
		state: string;
		outcome: string | null;
		inputAnswer?: string | null;
		report?: { inputRequest?: { question: string; options: string[] } | null } | null;
	}[];
}
interface RunEvent {
	sequence: number;
	atMs: number;
	kind: { type: string; [field: string]: unknown };
}
type RunReply =
	| { type: "runs"; value: RunSnapshot[] }
	| { type: "snapshot"; value: RunSnapshot }
	| { type: "events"; value: RunEvent[] }
	| { type: "receipt"; value: { snapshot: RunSnapshot; event?: RunEvent } }
	| { type: "readiness"; value: string[] };

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
		<label class={s.phaseSelector}>
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
	const [showRuns, setShowRuns] = createSignal(false);
	const [showDesigner, setShowDesigner] = createSignal(false);
	const [runs, setRuns] = createSignal<RunSnapshot[]>([]);
	const [selectedRunId, setSelectedRunId] = createSignal<string | null>(null);
	const [run, setRun] = createSignal<RunSnapshot | null>(null);
	const [runEvents, setRunEvents] = createSignal<RunEvent[]>([]);
	const [runLoading, setRunLoading] = createSignal(false);
	const [runError, setRunError] = createSignal("");
	const [inputAnswer, setInputAnswer] = createSignal("");
	const [startOpen, setStartOpen] = createSignal(false);
	const [publishedChoices, setPublishedChoices] = createSignal<
		{ id: string; name: string; latestPublishedRevision: number }[]
	>([]);
	const [workflowChoice, setWorkflowChoice] = createSignal("");
	const [startProblems, setStartProblems] = createSignal<string[]>([]);
	// The definition whose readiness answer is in; Start waits for it.
	const [readyFor, setReadyFor] = createSignal("");
	const [recoveryExecution, setRecoveryExecution] = createSignal("");
	const [recoveryActivation, setRecoveryActivation] = createSignal("");
	const [resolution, setResolution] = createSignal("");
	let startRequestId = "";
	createEffect(on(storyId, () => setStartOpen(false)));
	createEffect(
		on(selectedRunId, () => {
			setRecoveryExecution("");
			setRecoveryActivation("");
			setResolution("");
		}),
	);
	// One pending activation leaves nothing to choose: offer it, the operator still writes the resolution.
	createEffect(() => {
		const current = run();
		if (current?.status !== "paused" || untrack(recoveryExecution)) return;
		const pending = (current.graphExecutions ?? []).flatMap((graph) =>
			graph.activations
				.filter((activation) => activation.state === "ready" || activation.state === "running")
				.map((activation) => ({ execution: graph.id, activation: activation.id })),
		);
		if (pending.length !== 1) return;
		setRecoveryExecution(pending[0].execution);
		setRecoveryActivation(pending[0].activation);
	});
	const cancelDragOnBlur = () => cancelStoryDrag?.();
	onMount(() => window.addEventListener("blur", cancelDragOnBlur));
	onCleanup(() => window.removeEventListener("blur", cancelDragOnBlur));
	let closeButton: HTMLButtonElement | undefined;
	let request = 0;
	let runRequest = 0;
	onCleanup(() => {
		++request;
		++runRequest;
	});
	let capabilitiesReady = false;
	// A dependency choice belongs to the story it was made for.
	createEffect(on(storyId, () => setDependencyId("")));

	function changeFilters(): void {
		void refresh();
	}

	const selectedPlan = () => plans().find((plan) => plan.id === planId());
	const selectedStory = () => stories().find((story) => story.id === storyId());
	const storyPlan = () => plans().find((plan) => plan.id === selectedStory()?.planId);
	const runPlanId = () => selectedPlan()?.id ?? storyPlan()?.id;
	const planTitleFor = (story: Story) => plans().find((plan) => plan.id === story.planId)?.title ?? story.planId;
	const pendingQuestion = () => run()?.questions?.find((question) => question.state === "pending");
	const pendingInput = () => {
		const selected = run();
		if (selected?.questions) {
			const question = pendingQuestion();
			return selected.attempts.find((attempt) => attempt.id === question?.attemptId);
		}
		return selected?.attempts.find(
			(attempt) => attempt.outcome === "needs_input" && !attempt.inputAnswer && attempt.report?.inputRequest,
		);
	};
	const pendingOptions = () => {
		const question = pendingQuestion();
		return question ? (question.schema.options ?? []) : (pendingInput()?.report?.inputRequest?.options ?? []);
	};

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
	const ensureCapabilities = async (): Promise<void> => {
		if (capabilitiesReady) return;
		try {
			if ((await invoke<unknown>("story_capabilities")) !== true) throw new MissingStoriesBackendError();
			capabilitiesReady = true;
		} catch (cause) {
			if (
				cause instanceof MissingStoriesBackendError ||
				isTauri() ||
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

	const callRun = (action: Record<string, unknown>) =>
		invoke<RunReply>("workflow_run_action", { project: props.project, action });

	async function loadRun(runId: string): Promise<void> {
		const current = ++runRequest;
		const after = untrack(() => (run()?.id === runId ? (runEvents().at(-1)?.sequence ?? 0) : 0));
		setRunLoading(true);
		setRunError("");
		try {
			const [snapshot, events] = await Promise.all([
				callRun({ action: "get", run_id: runId }),
				callRun({ action: "events", run_id: runId, after_sequence: after, limit: 100 }),
			]);
			if (snapshot.type !== "snapshot" || events.type !== "events") throw new Error("Invalid run response");
			if (current !== runRequest) return;
			setRun(snapshot.value);
			setRuns((previous) => previous.map((item) => (item.id === snapshot.value.id ? snapshot.value : item)));
			setRunEvents((previous) => (after ? [...previous, ...events.value] : events.value));
		} catch (cause) {
			if (current === runRequest) setRunError(String(cause));
		} finally {
			if (current === runRequest) setRunLoading(false);
		}
	}

	async function openRuns(): Promise<void> {
		const currentPlan = runPlanId();
		if (!currentPlan) return;
		if (run()?.planId !== currentPlan) {
			++runRequest;
			setRuns([]);
			setSelectedRunId(null);
			setRun(null);
			setRunEvents([]);
		}
		setShowRuns(true);
		setRunLoading(true);
		setRunError("");
		try {
			const reply = await callRun({ action: "list_plan_runs", plan_id: currentPlan, limit: 20 });
			if (reply.type !== "runs") throw new Error("Invalid run list response");
			if (currentPlan !== runPlanId() || !showRuns()) return;
			setRuns(reply.value);
			const selected = reply.value.find((item) => item.id === selectedRunId())?.id ?? reply.value[0]?.id ?? null;
			const unchanged = selected === selectedRunId();
			setSelectedRunId(selected);
			if (!selected) {
				setRun(null);
				setRunEvents([]);
				setRunLoading(false);
			} else if (unchanged) await loadRun(selected);
		} catch (cause) {
			setRunError(String(cause));
			setRunLoading(false);
		}
	}

	async function answerRunInput(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const selected = run();
		const attempt = pendingInput();
		const answer = inputAnswer().trim();
		if (!selected || !attempt || !answer) return;
		setRunLoading(true);
		setRunError("");
		try {
			const question = pendingQuestion();
			const reply = await callRun({
				action: "command",
				run_id: selected.id,
				command_id: question ? `answer-question:${question.id}:${question.revision}` : `answer-input:${attempt.id}`,
				expected_sequence: selected.sequence,
				command: question
					? {
							action: "answer_question",
							question_id: question.id,
							expected_revision: question.revision,
							subject_digest: question.subjectDigest,
							actor: { kind: "human", id: "stories-dialog" },
							answer:
								question.schema.type === "boolean" || question.schema.type === "multiple_choice"
									? JSON.parse(answer)
									: answer,
						}
					: { action: "answer_input", attempt_id: attempt.id, answer },
			});
			if (reply.type !== "receipt") throw new Error("Invalid run response");
			setRun(reply.value.snapshot);
			setInputAnswer("");
		} catch (cause) {
			setRunError(String(cause));
		} finally {
			setRunLoading(false);
		}
	}

	async function controlRun(command: Record<string, unknown>): Promise<void> {
		const selected = run();
		if (!selected) return;
		setRunLoading(true);
		setRunError("");
		try {
			const reply = await callRun({
				action: "command",
				run_id: selected.id,
				command_id: crypto.randomUUID(),
				expected_sequence: selected.sequence,
				command,
			});
			if (selectedRunId() !== selected.id) return;
			if (reply.type !== "receipt") throw new Error("Invalid run response");
			setRun(reply.value.snapshot);
			setRecoveryExecution("");
			setRecoveryActivation("");
			setResolution("");
			await loadRun(selected.id);
		} catch (cause) {
			if (selectedRunId() === selected.id) setRunError(String(cause));
		} finally {
			if (selectedRunId() === selected.id) setRunLoading(false);
		}
	}

	async function checkStartReadiness(definitionId: string): Promise<void> {
		const workflow = publishedChoices().find((item) => item.id === definitionId);
		setStartProblems([]);
		setReadyFor("");
		if (!workflow) return;
		try {
			const reply = await callRun({
				action: "start_readiness",
				definition_id: workflow.id,
				definition_revision: workflow.latestPublishedRevision,
			});
			if (reply.type !== "readiness") throw new Error("Invalid run response");
			if (workflowChoice() === definitionId) {
				setStartProblems(reply.value);
				setReadyFor(definitionId);
			}
		} catch (cause) {
			if (workflowChoice() === definitionId) setStartProblems([`Could not check the start requirements: ${cause}`]);
		}
	}

	async function openStart(): Promise<void> {
		setBusy(true);
		setError("");
		const selected = storyId();
		try {
			const reply = await invoke<{
				type: string;
				value: { id: string; name: string; kind: string; latestPublishedRevision: number | null }[];
			}>("workflow_definition_action", { project: props.project, action: { action: "list_drafts" } });
			if (reply.type !== "drafts") throw new Error("Invalid workflow response");
			if (selected !== storyId()) return;
			const choices = reply.value.filter(
				(item): item is typeof item & { latestPublishedRevision: number } =>
					item.kind === "story" && item.latestPublishedRevision !== null,
			);
			setPublishedChoices(choices);
			setWorkflowChoice(choices[0]?.id ?? "");
			startRequestId = crypto.randomUUID();
			setStartOpen(true);
			void checkStartReadiness(choices[0]?.id ?? "");
		} catch (cause) {
			fail(cause);
		} finally {
			setBusy(false);
		}
	}

	async function startStoryWorkflow(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const selected = selectedStory();
		const workflow = publishedChoices().find((item) => item.id === workflowChoice());
		if (!selected || !workflow) return;
		setBusy(true);
		setError("");
		try {
			const reply = await callRun({
				action: "start_graph",
				target: { type: "story", id: selected.id },
				expected_revision: selected.revision,
				definition_id: workflow.id,
				definition_revision: workflow.latestPublishedRevision,
				request_id: startRequestId,
			});
			if (reply.type !== "snapshot") throw new Error("Invalid run response");
			if (selected.id !== storyId()) return;
			setStartOpen(false);
			setShowDesigner(false);
			setShowRuns(true);
			setRuns((previous) => [reply.value, ...previous.filter((item) => item.id !== reply.value.id)]);
			const unchanged = selectedRunId() === reply.value.id;
			setRun(null);
			setRunEvents([]);
			setSelectedRunId(reply.value.id);
			if (unchanged) await loadRun(reply.value.id);
		} catch (cause) {
			fail(cause);
		} finally {
			setBusy(false);
		}
	}

	createEffect(() => {
		const runId = selectedRunId();
		if (!showRuns() || !runId) return;
		workflowRunSignals.sequence(selectedPlan()?.project ?? props.project, runId);
		workflowRunSignals.resyncRevision();
		void loadRun(runId);
	});

	async function refresh(preferredPlan = planId(), preferredStory = storyId()): Promise<void> {
		const current = ++request;
		setLoading(true);
		setError("");
		try {
			await ensureCapabilities();
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
			if (current !== request) return;
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

	onMount(() => {
		closeButton?.focus();
		void refresh();
	});

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
						<span class={s.project} title={props.project}>
							{props.project}
						</span>
					</div>
					<div class={s.headerActions}>
						<Button
							size="compact"
							type="button"
							aria-pressed={!showRuns() && !showDesigner()}
							onClick={() => {
								setShowRuns(false);
								setShowDesigner(false);
							}}
						>
							Stories
						</Button>
						<Button
							size="compact"
							type="button"
							disabled={
								busy() ||
								storyPlan()?.archived ||
								selectedStory()?.archived ||
								selectedStory()?.status !== "ready" ||
								!!selectedStory()?.claimSession
							}
							onClick={() => void openStart()}
						>
							Start story workflow
						</Button>
						{/* DEFERRED (2026-10-22) — "Start plan workflow" is hidden until the slice E plan dispatch entry point exists (audit 2137 A11). */}
						<Button
							size="compact"
							type="button"
							aria-pressed={showRuns()}
							disabled={!runPlanId()}
							onClick={() => {
								setShowDesigner(false);
								void openRuns();
							}}
						>
							Run history
						</Button>
						<Button
							size="compact"
							type="button"
							aria-pressed={showDesigner()}
							onClick={() => {
								setShowRuns(false);
								setShowDesigner(true);
							}}
						>
							Designer
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
				<Show when={startOpen()}>
					<form class={s.form} onSubmit={(event) => void startStoryWorkflow(event)}>
						<p>Start · {selectedStory()?.title}</p>
						<Show
							when={publishedChoices().length > 0}
							fallback={<p>No published story workflow. Publish one in Designer.</p>}
						>
							<label>
								Published workflow
								<Select
									value={workflowChoice()}
									onChange={(event) => {
										setWorkflowChoice(event.currentTarget.value);
										startRequestId = crypto.randomUUID();
										void checkStartReadiness(event.currentTarget.value);
									}}
								>
									<For each={publishedChoices()}>
										{(workflow) => (
											<option value={workflow.id}>
												{workflow.name} · revision {workflow.latestPublishedRevision}
											</option>
										)}
									</For>
								</Select>
							</label>
							<Show when={startProblems().length > 0}>
								<div role="alert" class={s.error}>
									<For each={startProblems()}>{(problem) => <p>{problem}</p>}</For>
								</div>
							</Show>
							<Button
								variant="primary"
								type="submit"
								disabled={busy() || readyFor() !== workflowChoice() || startProblems().length > 0}
							>
								Start published workflow
							</Button>
						</Show>
						<Button type="button" onClick={() => setStartOpen(false)}>
							Dismiss
						</Button>
					</form>
				</Show>
				<Show when={showDesigner()}>
					<WorkflowDesigner project={props.project} />
				</Show>
				<Show when={showRuns()}>
					<div class={s.runContent}>
						<aside class={s.runList} aria-label="Plan runs">
							<div class={s.columnHeader}>
								<h3>
									Runs ·{" "}
									{plans().find((plan) => plan.id === run()?.planId)?.title ??
										storyPlan()?.title ??
										selectedPlan()?.title}
								</h3>
								<Button type="button" onClick={() => void openRuns()}>
									Refresh
								</Button>
							</div>
							<Show when={!runLoading() && runs().length === 0}>
								<p class={s.empty}>No workflow runs for this plan.</p>
							</Show>
							<nav class={s.items}>
								<For each={runs()}>
									{(item) => (
										<button
											data-control-layout="row"
											type="button"
											class={s.item}
											aria-current={selectedRunId() === item.id ? "true" : undefined}
											onClick={() => setSelectedRunId(item.id)}
										>
											<span>{new Date(item.startedMs).toLocaleString()}</span>
											<small>
												{item.status} · {item.stories.length} {item.stories.length === 1 ? "story" : "stories"} · #
												{item.sequence}
											</small>
										</button>
									)}
								</For>
							</nav>
						</aside>
						<section class={s.runDetail} aria-label="Run timeline">
							<Show when={runError()}>
								<p class={s.error} role="alert">
									{runError()}
								</p>
							</Show>
							<Show when={runLoading()}>
								<p class={s.message} role="status">
									Loading run…
								</p>
							</Show>
							<Show when={run()}>
								{(selected) => (
									<>
										<div class={s.detailHeader}>
											<div>
												<span class={s.eyebrow}>Run · {selected().id}</span>
												<h3>{selectedPlan()?.title}</h3>
											</div>
											<span class={s.status} data-status={selected().status}>
												{selected().status.replaceAll("_", " ")}
											</span>
										</div>
										<p class={s.muted}>
											{selected().stories.length} {selected().stories.length === 1 ? "story" : "stories"} ·{" "}
											{selected().attempts.length} {selected().attempts.length === 1 ? "attempt" : "attempts"} ·
											sequence {selected().sequence}
										</p>
										<RunIncidents project={props.project} runId={selected().id} sequence={selected().sequence} />
										<For
											each={selected().questions?.filter(
												(question) => question.state === "timed_out" || question.state === "incomplete",
											)}
										>
											{(question) => (
												<p class={s.muted}>
													{question.request.question} · {question.state.replaceAll("_", " ")}
												</p>
											)}
										</For>
										<Show when={pendingInput()}>
											{(attempt) => (
												<form class={s.form} onSubmit={(event) => void answerRunInput(event)}>
													<p>{attempt().report?.inputRequest?.question}</p>
													<label>
														Answer
														<Input
															value={inputAnswer()}
															onInput={(event) => setInputAnswer(event.currentTarget.value)}
														/>
													</label>
													<Show when={pendingOptions().length}>
														<small>Suggested: {pendingOptions().join(", ")}</small>
													</Show>
													<Button variant="primary" type="submit" disabled={runLoading() || !inputAnswer().trim()}>
														Record answer
													</Button>
												</form>
											)}
										</Show>

										<Show when={!selected().graphExecutions?.length}>
											<p class={s.muted}>Legacy run · inspect or cancel only.</p>
										</Show>
										<Show when={selected().status === "running" && !!selected().graphExecutions?.length}>
											<Button
												type="button"
												class={s.loadMore}
												disabled={runLoading()}
												onClick={() => void controlRun({ action: "pause" })}
											>
												Pause run
											</Button>
										</Show>
										<Show when={selected().status === "running" || selected().status === "paused"}>
											<Button
												type="button"
												class={s.loadMore}
												disabled={runLoading()}
												onClick={() => void controlRun({ action: "cancel" })}
											>
												Cancel run
											</Button>
										</Show>
										<Show when={selected().status === "paused" && selected().pauseCause}>
											<p role="status">Paused: {selected().pauseCause}</p>
										</Show>
										<For each={selected().graphExecutions ?? []}>
											{(graph) => (
												<section aria-label={`Graph ${graph.id}`}>
													<h4>
														{graph.id} · {graph.targetId} · revision {graph.definition.revision}
													</h4>
													<For each={graph.activations}>
														{(activation) => (
															<p>
																{activation.id} · {activation.nodeId} ·{" "}
																{selected().status === "cancelled" && activation.state !== "completed"
																	? "cancelled"
																	: activation.state}
															</p>
														)}
													</For>
													<For each={graph.decisions}>
														{(decision) => (
															<p>
																Decision · {decision.activationId} ·{" "}
																{graph.definition.graph.edges[decision.edgeIndex]?.outcome} · {decision.evidence.actor}{" "}
																· {decision.evidence.reason} · {decision.evidence.references.join(", ")}
															</p>
														)}
													</For>
													<For each={graph.loops}>
														{(loop) => (
															<p>
																Loop · {loop.nodeId} · {loop.repeats} repeats
															</p>
														)}
													</For>
													<For each={graph.pauses}>
														{(pause) => (
															<p>
																Pause · {pause.activationId} · {pause.evidence.reason} · resume to {pause.resumeTo} ·{" "}
																{pause.resolution ?? "unresolved"}
															</p>
														)}
													</For>
												</section>
											)}
										</For>
										<Show
											when={selected().status === "paused" && !!selected().graphExecutions?.length && !pendingInput()}
										>
											<form
												class={s.form}
												onSubmit={(event) => {
													event.preventDefault();
													void controlRun({
														action: "resume_graph",
														execution_id: recoveryExecution(),
														activation_id: recoveryActivation(),
														resolution: resolution().trim(),
													});
												}}
											>
												<label>
													Execution
													<Select
														value={recoveryExecution()}
														onChange={(event) => {
															setRecoveryExecution(event.currentTarget.value);
															setRecoveryActivation("");
														}}
													>
														<option value="">Select execution</option>
														<For each={selected().graphExecutions ?? []}>
															{(graph) => (
																<option value={graph.id}>
																	{graph.id} · {graph.targetId}
																</option>
															)}
														</For>
													</Select>
												</label>
												<label>
													Resume activation
													<Select
														value={recoveryActivation()}
														onChange={(event) => setRecoveryActivation(event.currentTarget.value)}
													>
														<option value="">Select activation</option>
														<For
															each={
																selected()
																	.graphExecutions?.find((graph) => graph.id === recoveryExecution())
																	?.activations.filter((activation) => activation.state !== "completed") ?? []
															}
														>
															{(activation) => (
																<option value={activation.id}>
																	{activation.id} · {activation.nodeId} · {activation.state}
																</option>
															)}
														</For>
													</Select>
												</label>
												<label>
													Resolution
													<Textarea
														value={resolution()}
														onInput={(event) => setResolution(event.currentTarget.value)}
														maxLength={4096}
													/>
												</label>
												<Button
													variant="primary"
													type="submit"
													disabled={
														runLoading() || !recoveryExecution() || !recoveryActivation() || !resolution().trim()
													}
												>
													Resume run
												</Button>
											</form>
										</Show>
										<ol class={s.timeline}>
											<For each={runEvents()}>
												{(event) => (
													<li>
														<span class={s.eventSequence}>#{event.sequence}</span>
														<div>
															<span>{event.kind.type.replaceAll("_", " ")}</span>
															<details>
																<summary>Event details</summary>
																<pre>{JSON.stringify(event.kind, null, 2)}</pre>
															</details>
														</div>
														<time>{new Date(event.atMs).toLocaleTimeString()}</time>
													</li>
												)}
											</For>
										</ol>
										<Show when={(runEvents().at(-1)?.sequence ?? 0) < selected().sequence}>
											<Button
												type="button"
												class={s.loadMore}
												disabled={runLoading()}
												onClick={() => void loadRun(selected().id)}
											>
												Load more events
											</Button>
										</Show>
									</>
								)}
							</Show>
						</section>
					</div>
				</Show>
				<Show when={!showRuns() && !showDesigner()}>
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
											disabled={busy()}
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
													disabled={busy()}
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
										<For
											each={planSources().filter((source) => !plans().some((plan) => plan.source === source.source))}
										>
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
										disabled={!selectedPlan() || selectedPlan()?.archived}
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
																{statusLabel(story.status)} · {storyTypeLabel(story.storyType)}
															</small>
														</button>
														<PhaseSelector
															story={story}
															choices={phaseChoices().get(story.id)}
															disabled={busy() || loading() || !!editing()}
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
															disabled={busy()}
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
															disabled={busy() || loading()}
															label={t("stories.phase.label", "Phase")}
															onMove={moveStory}
														/>
													</Show>
												</div>
											</div>
											<StoryMetadata project={props.project} story={story()} />
											<Show when={editing()} keyed>
												{(snapshot) => (
													<StoryEditForm
														story={snapshot}
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
																					disabled={busy()}
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
																disabled={!!dependencyReason()}
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
													<Show when={!story().archived && !storyPlan()?.archived}>
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
														disabled={busy() || !!story().claimSession}
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
															busy={busy()}
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
				</Show>
			</div>
		</div>
	);
};
