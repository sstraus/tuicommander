import type { JsonValue, ObjectSchema } from "./catalogTypes";
import type { Diagnostic, NeutralPublication, PublicationRef } from "./neutralClient";

export interface ResourceWait {
	resource: string;
	owner_run: string;
	owner_effect: string;
}
export interface RunView {
	resource_waits?: ResourceWait[];
	id: string;
	publication: PublicationRef;
	name: string;
	status: { type: string };
	current_node: string;
	waiting_on_approval: boolean;
	started_ms: number;
	updated_ms: number;
}
export interface Approval {
	key: {
		fence: { run: string; execution: string; activation: number; attempt: number; scope: JsonValue };
		digest: string;
	};
	question: { revision: number; title: string; deadline_ms: number };
	operator_request: JsonValue;
}
export type NodeState = "completed" | "current" | "waiting" | "resource_wait" | "timer_wait" | "failed" | "stopped";
export interface RunProgress {
	server_now_ms?: number;
	resource_waits?: ResourceWait[];
	name: string;
	node_states: Record<string, NodeState>;
	waiting_on_approval: boolean;
	approvals: Approval[];
	publication: NeutralPublication;
	started_ms: number;
	updated_ms: number;
}

export interface Datum {
	state: { state: "present"; value: JsonValue } | { state: "unknown" | "absent" };
	provenance: JsonValue[];
}
export interface Preview {
	validation: { valid: boolean; diagnostics: Diagnostic[] };
	activities: {
		activity: number;
		inputs: Record<string, Datum>;
		outputs: ObjectSchema;
		outcomes: string[];
		effect_intent: JsonValue;
		unavailable_reason: string | null;
	}[];
	outputs: ObjectSchema;
}
export interface Inspection {
	progress?: RunProgress;
	snapshot: Record<string, JsonValue>;
	events: (Record<string, JsonValue> & { sequence: number })[];
}
