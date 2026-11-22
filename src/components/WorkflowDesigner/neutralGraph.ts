import type { ActivityDescriptor } from "./catalogTypes";
import type { ControlNode, NeutralDefinition } from "./neutralClient";

/** Presentation only: routes and semantics remain the backend's neutral graph. */
export function nodePorts(
	node: ControlNode,
	definition: NeutralDefinition,
	activities: ActivityDescriptor[],
): string[] {
	switch (node.type) {
		case "start":
		case "wait":
			return ["next"];
		case "end":
			return [];
		case "condition":
			return ["true", "false", "unknown"];
		case "collection":
		case "fork":
			return ["complete", "blocked"];
		case "loop":
			return ["complete", "exhausted", "unknown", "failed"];
		case "activity": {
			const id = definition.instances[node.activity]?.descriptor;
			return (
				activities.find(
					(item) => item.id.namespace === id?.namespace && item.id.name === id?.name && item.id.version === id?.version,
				)?.outcomes ?? []
			);
		}
	}
}

export function nodeTitle(id: string, definition: NeutralDefinition, labels: Record<string, string>): string {
	if (labels[id]) return labels[id];
	const node = definition.control.nodes[id];
	if (!node) return "Missing node";
	if (node.type === "activity")
		return `${definition.instances[node.activity]?.descriptor.name ?? "Activity"} ${node.activity + 1}`;
	if (node.type === "end") return `End · ${node.outcome}`;
	if (node.type === "start") return "Start";
	const peers = Object.entries(definition.control.nodes).filter(([, item]) => item.type === node.type);
	return `${node.type[0].toUpperCase()}${node.type.slice(1)} ${peers.findIndex(([key]) => key === id) + 1}`;
}

/** The title of the node that runs an activity, so bindings name it as the graph does. */
export function activityTitle(definition: NeutralDefinition, labels: Record<string, string>, ordinal: number): string {
	const id = Object.keys(definition.control.nodes).find((key) => {
		const node = definition.control.nodes[key];
		return node.type === "activity" && node.activity === ordinal;
	});
	return id
		? nodeTitle(id, definition, labels)
		: `${definition.instances[ordinal]?.descriptor.name ?? "Activity"} ${ordinal + 1}`;
}

export function connectRoute(
	definition: NeutralDefinition,
	id: string,
	port: string,
	target: string,
): NeutralDefinition {
	const { [port]: _removed, ...remaining } = definition.control.edges[id] ?? {};
	return {
		...definition,
		control: {
			...definition.control,
			edges: {
				...definition.control.edges,
				[id]: target ? { ...remaining, [port]: target } : remaining,
			},
		},
	};
}

export const GRAPH_NODE_WIDTH = 196;
export const GRAPH_NODE_HEIGHT = 84;
/** Stable layered DAG layout; corrupt/cyclic imports still render every node. */
export function graphLayout(definition: NeutralDefinition) {
	const ids = Object.keys(definition.control.nodes);
	const indegree = new Map(ids.map((id) => [id, 0]));
	const level = new Map(ids.map((id) => [id, 0]));
	const targets = (id: string) =>
		[...new Set(Object.values(definition.control.edges[id] ?? {}))].filter((target) => indegree.has(target));
	for (const id of ids) for (const target of targets(id)) indegree.set(target, (indegree.get(target) ?? 0) + 1);
	const queue = ids.filter((id) => indegree.get(id) === 0);
	const visited = new Set<string>();
	for (let i = 0; i < queue.length; i++) {
		const id = queue[i];
		visited.add(id);
		for (const target of targets(id)) {
			level.set(target, Math.max(level.get(target) ?? 0, (level.get(id) ?? 0) + 1));
			indegree.set(target, (indegree.get(target) ?? 0) - 1);
			if (indegree.get(target) === 0) queue.push(target);
		}
	}
	let last = Math.max(0, ...level.values());
	for (const id of ids) if (!visited.has(id)) level.set(id, ++last);
	const rows = new Map<number, string[]>();
	for (const id of ids) {
		const rank = level.get(id) ?? 0;
		rows.set(rank, [...(rows.get(rank) ?? []), id]);
	}
	const width = Math.max(1, ...[...rows.values()].map((row) => row.length)) * 252 + 80;
	const positions: Record<string, { x: number; y: number }> = {};
	for (const [rank, row] of rows)
		row.forEach((id, index) => {
			positions[id] = { x: (width - row.length * 252) / 2 + index * 252 + 28, y: rank * 144 + 40 };
		});
	return { positions, width, height: (Math.max(0, ...level.values()) + 1) * 144 + 40 };
}
