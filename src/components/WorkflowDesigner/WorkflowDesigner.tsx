import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { HttpRpcError } from "../../transport";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Textarea } from "../ui/Textarea";
import s from "./WorkflowDesigner.module.css";

type Role = "coordinator" | "planner" | "implementer" | "reviewer" | "validator";
type Kind =
	| { type: "start" | "create_stories" | "judge" | "gate" | "join" | "notify" | "end" }
	| { type: "agent"; role: Role; capabilities: string[]; prompt_template: string }
	| { type: "pause"; resume_to?: string | null }
	| { type: "loop"; max_iterations: number }
	| { type: "story_dispatch"; story_template_id: string; story_revision: number };
interface Node {
	id: string;
	kind: Kind;
}
interface Edge {
	from: string;
	to: string;
	outcome: string | null;
}
interface Graph {
	nodes: Node[];
	edges: Edge[];
}
interface Draft {
	id: string;
	project: string;
	name: string;
	kind: "plan" | "story";
	graph: Graph;
	draftRevision: number;
	latestPublishedRevision: number;
	lastPublishedDraftRevision: number;
	builtinKey: string | null;
}
type Reply =
	| { type: "drafts"; value: Draft[] }
	| { type: "draft"; value: Draft }
	| { type: "published"; value: { revision: number } };
type PaletteItem = { label: string; type: Kind["type"]; role?: Role };
const PALETTE: PaletteItem[] = [
	{ label: "Planner", type: "agent", role: "planner" },
	{ label: "Implement", type: "agent", role: "implementer" },
	{ label: "Review", type: "agent", role: "reviewer" },
	{ label: "Validate", type: "agent", role: "validator" },
	{ label: "Judge", type: "judge" },
	{ label: "Gate", type: "gate" },
	{ label: "Pause", type: "pause" },
	{ label: "Loop", type: "loop" },
	{ label: "Notify", type: "notify" },
	{ label: "Join", type: "join" },
];

function makeKind(item: PaletteItem): Kind {
	if (item.type === "agent") {
		const role = item.role ?? "implementer";
		return {
			type: "agent",
			role,
			capabilities:
				role === "planner" || role === "coordinator"
					? ["story_read", "story_create", "agent_spawn"]
					: ["story_read", "story_report"],
			prompt_template: `${role} {{story.id}} and report evidence.`,
		};
	}
	if (item.type === "loop") return { type: "loop", max_iterations: 3 };
	if (item.type === "story_dispatch") throw new Error("Story dispatch needs a published story template");
	return { type: item.type };
}
function copyGraph(graph: Graph): Graph {
	return {
		nodes: graph.nodes.map((node) => ({ ...node, kind: { ...node.kind } })),
		edges: graph.edges.map((edge) => ({ ...edge })),
	};
}
function outcomes(kind: Kind): string[] {
	switch (kind.type) {
		case "judge":
			return ["yes", "no", "uncertain"];
		case "gate":
			return ["pass", "fail"];
		case "loop":
			return ["repeat", "exhausted"];
		case "story_dispatch":
			return ["completed", "blocked"];
		default:
			return [];
	}
}

/** An HTTP failure carries the server's plain message in `detail`; `String(cause)` would show the RPC envelope. */
function plainMessage(cause: unknown): string {
	return cause instanceof HttpRpcError ? cause.detail : String(cause);
}

export interface WorkflowDesignerProps {
	project: string;
}
export const WorkflowDesigner: Component<WorkflowDesignerProps> = (props) => {
	const [drafts, setDrafts] = createSignal<Draft[]>([]);
	const [draft, setDraft] = createSignal<Draft | null>(null);
	const [graph, setGraph] = createSignal<Graph>({ nodes: [], edges: [] });
	const [selectedId, setSelectedId] = createSignal<string | null>(null);
	const [targetId, setTargetId] = createSignal("");
	const [outcome, setOutcome] = createSignal("");
	const [dirty, setDirty] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [notice, setNotice] = createSignal("");
	const selected = () => graph().nodes.find((node) => node.id === selectedId());
	const call = (action: Record<string, unknown>) =>
		invoke<Reply>("workflow_definition_action", { project: props.project, action });

	function chooseDraft(next: Draft): void {
		if (dirty() && draft()?.id !== next.id) {
			setError("Save the current draft before switching.");
			return;
		}
		setDraft(next);
		setGraph(copyGraph(next.graph));
		setSelectedId(next.graph.nodes[0]?.id ?? null);
		setTargetId("");
		setOutcome("");
		setDirty(false);
		setError("");
		setNotice("");
	}
	async function load(): Promise<void> {
		setBusy(true);
		setError("");
		try {
			const reply = await call({ action: "list_drafts" });
			if (reply.type !== "drafts") throw new Error("Invalid workflow response");
			setDrafts(reply.value);
			if (reply.value.length) chooseDraft(reply.value.find((item) => item.id === draft()?.id) ?? reply.value[0]);
		} catch (cause) {
			setError(plainMessage(cause));
		} finally {
			setBusy(false);
		}
	}
	function addNode(item: PaletteItem): void {
		const base = item.role ?? item.type;
		let index = 1;
		while (graph().nodes.some((node) => node.id === `${base}_${index}`)) index += 1;
		const id = `${base}_${index}`;
		setGraph((current) => ({ ...current, nodes: [...current.nodes, { id, kind: makeKind(item) }] }));
		setSelectedId(id);
		setDirty(true);
		setNotice("");
	}
	function connect(): void {
		const source = selected();
		const target = targetId();
		if (!source || !target || source.id === target) return;
		const choices = outcomes(source.kind);
		if (choices.length && !choices.includes(outcome())) {
			setError("Choose an outcome for this connection.");
			return;
		}
		const edge: Edge = { from: source.id, to: target, outcome: choices.length ? outcome() : null };
		setGraph((current) => ({
			...current,
			edges: [...current.edges.filter((item) => !(item.from === edge.from && item.outcome === edge.outcome)), edge],
		}));
		setDirty(true);
		setError("");
	}
	function removeNode(): void {
		const id = selectedId();
		if (!id || selected()?.kind.type === "start") return;
		setGraph((current) => ({
			nodes: current.nodes.filter((node) => node.id !== id),
			edges: current.edges.filter((edge) => edge.from !== id && edge.to !== id),
		}));
		setSelectedId(graph().nodes[0]?.id ?? null);
		setDirty(true);
	}
	function updateKind(kind: Kind): void {
		const id = selectedId();
		setGraph((current) => ({
			...current,
			nodes: current.nodes.map((node) => (node.id === id ? { ...node, kind } : node)),
		}));
		setDirty(true);
	}
	async function save(): Promise<void> {
		const current = draft();
		if (!current || !dirty()) return;
		setBusy(true);
		setError("");
		try {
			const reply = await call({
				action: "update_draft",
				id: current.id,
				expected_revision: current.draftRevision,
				graph: graph(),
			});
			if (reply.type !== "draft") throw new Error("Invalid workflow response");
			setDraft(reply.value);
			setDrafts((items) => items.map((item) => (item.id === reply.value.id ? reply.value : item)));
			setDirty(false);
			setNotice("Draft saved.");
		} catch (cause) {
			setError(plainMessage(cause));
		} finally {
			setBusy(false);
		}
	}
	async function publish(): Promise<void> {
		const current = draft();
		if (!current || dirty()) return;
		setBusy(true);
		setError("");
		try {
			const reply = await call({ action: "publish", id: current.id, expected_revision: current.draftRevision });
			if (reply.type !== "published") throw new Error("Invalid workflow response");
			setNotice(`Published revision ${reply.value.revision}.`);
			setDraft({
				...current,
				latestPublishedRevision: reply.value.revision,
				lastPublishedDraftRevision: current.draftRevision,
			});
		} catch (cause) {
			setError(plainMessage(cause));
		} finally {
			setBusy(false);
		}
	}
	onMount(() => void load());
	return (
		<section class={s.designer} aria-label="Workflow designer">
			<header class={s.header}>
				<div>
					<span class={s.eyebrow}>Flow definitions</span>
					<h3>Workflow designer</h3>
				</div>
				<div class={s.actions}>
					<Button type="button" onClick={() => void load()} disabled={busy() || dirty()}>
						Refresh
					</Button>
					<Button type="button" onClick={() => void save()} disabled={busy() || !dirty()}>
						Save draft
					</Button>
					<Button
						type="button"
						onClick={() => void publish()}
						disabled={busy() || dirty() || !draft() || draft()?.draftRevision === draft()?.lastPublishedDraftRevision}
					>
						Publish
					</Button>
				</div>
			</header>
			<Show when={error()}>
				<p class={s.error} role="alert">
					{error()}
				</p>
			</Show>
			<Show when={notice()}>
				<p class={s.notice} role="status">
					{notice()}
				</p>
			</Show>
			<div class={s.body}>
				<aside class={s.sidebar} aria-label="Workflow drafts">
					<h4>Drafts</h4>
					<For each={drafts()}>
						{(item) => (
							<button
								type="button"
								class={s.draft}
								data-control-layout="row"
								aria-current={draft()?.id === item.id ? "true" : undefined}
								onClick={() => chooseDraft(item)}
							>
								<strong>{item.name}</strong>
								<small>
									{item.kind} · draft {item.draftRevision} · published {item.latestPublishedRevision}
								</small>
							</button>
						)}
					</For>
					<h4>Nodes</h4>
					<div class={s.palette}>
						<For each={PALETTE.filter((item) => item.type !== "join" || draft()?.kind === "story")}>
							{(item) => (
								<Button
									type="button"
									draggable="true"
									onDragStart={(event) =>
										event.dataTransfer?.setData("application/x-tuic-node", `${item.type}:${item.role ?? ""}`)
									}
									onClick={() => addNode(item)}
								>
									Add {item.label}
								</Button>
							)}
						</For>
					</div>
				</aside>
				<div
					class={s.canvas}
					aria-label="Workflow graph"
					onDragOver={(event) => event.preventDefault()}
					onDrop={(event) => {
						event.preventDefault();
						const value = event.dataTransfer?.getData("application/x-tuic-node") ?? "";
						const item = PALETTE.find((preset) => `${preset.type}:${preset.role ?? ""}` === value);
						if (item) addNode(item);
					}}
				>
					<Show when={draft()}>
						{(current) => (
							<div class={s.canvasTitle}>
								<span>{current().name}</span>
								<small>
									{current().kind} workflow · {graph().nodes.length} nodes · {graph().edges.length} links
								</small>
							</div>
						)}
					</Show>
					<div class={s.graph}>
						<For each={graph().nodes}>
							{(node) => (
								<div class={s.nodeColumn}>
									<button
										type="button"
										class={s.node}
										data-control-layout="card"
										aria-label={`Node ${node.id}`}
										data-node-id={node.id}
										aria-pressed={selectedId() === node.id}
										onClick={() => setSelectedId(node.id)}
									>
										<span class={s.nodeType}>
											{node.kind.type === "agent" ? node.kind.role : node.kind.type.replaceAll("_", " ")}
										</span>
										<strong>{node.id}</strong>
									</button>
									<div class={s.links}>
										<For each={graph().edges.filter((edge) => edge.from === node.id)}>
											{(edge) => (
												<span>
													{edge.outcome ? `${edge.outcome} → ` : "→ "}
													{edge.to}
												</span>
											)}
										</For>
									</div>
								</div>
							)}
						</For>
					</div>
				</div>
				<aside class={s.inspector} aria-label="Node inspector">
					<h4>Node inspector</h4>
					<Show when={selected()}>
						{(node) => (
							<>
								<p class={s.nodeName}>{node().id}</p>
								<p class={s.muted}>{node().kind.type}</p>
								<Show when={node().kind.type === "agent"}>
									<label>
										Prompt template
										<Textarea
											value={(node().kind as Extract<Kind, { type: "agent" }>).prompt_template}
											onInput={(event) =>
												updateKind({
													...(node().kind as Extract<Kind, { type: "agent" }>),
													prompt_template: event.currentTarget.value,
												})
											}
										/>
									</label>
								</Show>
								<Show when={node().kind.type === "pause"}>
									<label>
										Resume to
										<Select
											aria-label="Resume to"
											value={(node().kind as Extract<Kind, { type: "pause" }>).resume_to ?? ""}
											onChange={(event) => updateKind({ type: "pause", resume_to: event.currentTarget.value || null })}
										>
											<option value="">Choose node</option>
											<For each={graph().nodes}>
												{(target) => (
													<option
														value={target.id}
														disabled={target.kind.type === "start" || target.kind.type === "pause"}
													>
														{target.id}
													</option>
												)}
											</For>
										</Select>
									</label>
								</Show>
								<Show when={node().kind.type === "loop"}>
									<label>
										Maximum iterations
										<Input
											type="number"
											min="1"
											max="100"
											value={(node().kind as Extract<Kind, { type: "loop" }>).max_iterations}
											onInput={(event) =>
												updateKind({ type: "loop", max_iterations: Number(event.currentTarget.value) })
											}
										/>
									</label>
								</Show>
								<label>
									Connect to
									<Select
										aria-label="Connect to"
										value={targetId()}
										onChange={(event) => setTargetId(event.currentTarget.value)}
									>
										<option value="">Choose node</option>
										<For each={graph().nodes.filter((target) => target.id !== node().id)}>
											{(target) => <option value={target.id}>{target.id}</option>}
										</For>
									</Select>
								</label>
								<Show when={outcomes(node().kind).length}>
									<label>
										Outcome
										<Select
											aria-label="Outcome"
											value={outcome()}
											onChange={(event) => setOutcome(event.currentTarget.value)}
										>
											<option value="">Choose outcome</option>
											<For each={outcomes(node().kind)}>{(choice) => <option value={choice}>{choice}</option>}</For>
										</Select>
									</label>
								</Show>
								<Button type="button" onClick={connect} disabled={!targetId()}>
									Connect nodes
								</Button>
								<Button type="button" variant="danger" onClick={removeNode} disabled={node().kind.type === "start"}>
									Remove node
								</Button>
							</>
						)}
					</Show>
				</aside>
			</div>
		</section>
	);
};
