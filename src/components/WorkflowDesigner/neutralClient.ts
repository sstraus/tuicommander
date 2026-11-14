import { invoke } from "../../invoke";
import { HttpRpcError } from "../../transport";
import type { ActivityDescriptor, ActivityInstance, JsonValue, ObjectSchema } from "./catalogTypes";
import type { InputBindings } from "./MappingFields";
import type { Inspection, Preview } from "./neutralInspectionTypes";
import type {
	NeutralTriggerBinding,
	TriggerAdmission,
	TriggerBindingDraft,
	TriggerEvent,
	TriggerPreview,
	TriggerSource,
} from "./triggerTypes";

/** The readable reason of a failed backend call. */
export function plainMessage(cause: unknown): string {
	return cause instanceof HttpRpcError ? cause.detail : String(cause);
}

export type ControlNode =
	| { type: "start" }
	| { type: "end"; outcome: string }
	| { type: "activity"; activity: number }
	| { type: "condition"; condition: string }
	| { type: "collection" | "fork"; scope: string }
	| { type: "loop"; scope: string; condition: string }
	| { type: "wait"; duration_ms: number };
export interface PublicationRef {
	project: string;
	id: string;
	revision: number;
}
export interface NeutralDefinition {
	control: { nodes: Record<string, ControlNode>; edges: Record<string, Record<string, string>> };
	outputs: ObjectSchema;
	outcomes: string[];
	inputs: ObjectSchema;
	subject: { required: boolean; fields: ObjectSchema } | null;
	instances: ActivityInstance[];
	input_bindings: Record<string, InputBindings>;
	conditions: Record<string, JsonValue>;
	collections: Record<string, JsonValue>;
	loops: Record<string, JsonValue>;
	forks: Record<string, JsonValue>;
	output_bindings: InputBindings;
	subflows: PublicationRef[];
}
export interface EditorMetadata {
	labels: Record<string, string>;
	presets: Record<string, string>;
}
export interface NeutralDraft {
	editor: EditorMetadata;
	id: string;
	project: string;
	name: string;
	revision: number;
	latest_published_revision: number;
	definition: NeutralDefinition;
}
export interface NeutralPublication {
	editor: EditorMetadata;
	format_version: number;
	id: string;
	project: string;
	revision: number;
	definition: NeutralDefinition;
	pins: { pins: { id: ActivityDescriptor["id"]; sha256: string }[] };
	subflow_pins: { reference: PublicationRef; sha256: string }[];
}
export interface NeutralTemplate {
	key: string;
	name: string;
	description: string;
	editor: EditorMetadata;
	definition: NeutralDefinition;
}
export interface Diagnostic {
	path: string;
	code: string;
	message: string;
}
export interface Catalog {
	activities: ActivityDescriptor[];
	metanodes: NeutralPublication[];
}
export interface ActivityPreset {
	format_version: number;
	project: string;
	id: string;
	revision: number;
	instance: ActivityInstance;
	descriptor_pin: { id: ActivityDescriptor["id"]; sha256: string };
}
export interface CorrectionRegion {
	max_corrections: number;
	criteria: { criteria: string[] };
	selected: ActivityInstance;
}
export interface CorrectionRegionView {
	region: CorrectionRegion;
	definition: NeutralDefinition;
}
export interface NeutralReplies {
	correction_region: CorrectionRegionView;
	catalog: Catalog;
	drafts: NeutralDraft[];
	draft: NeutralDraft;
	templates: NeutralTemplate[];
	validation: { valid: boolean; diagnostics: Diagnostic[] };
	publication: NeutralPublication;
	preset: ActivityPreset;
	preview: Preview;
	inspection: Inspection;
	schema: ObjectSchema;
	trigger_bindings: NeutralTriggerBinding[];
	trigger_binding: NeutralTriggerBinding;
	trigger_preview: TriggerPreview;
	trigger_admission: TriggerAdmission[];
}
export type NeutralAction =
	| { action: "correction_region"; region: CorrectionRegion }
	| { action: "trigger_schema"; source: TriggerSource; kind: string }
	| { action: "trigger_bindings" }
	| { action: "save_trigger_binding"; expected_revision: number; draft: TriggerBindingDraft }
	| {
			action: "enable_trigger_binding";
			id: string;
			expected_revision: number;
			enabled: boolean;
			acknowledged_overlaps: Record<string, number>;
	  }
	| { action: "preview_trigger_mapping"; draft: TriggerBindingDraft; snapshot: TriggerEvent }
	| { action: "replay_trigger"; binding_id: string; resource: string; generation: string; replay_id: string }
	| { action: "catalog" | "list_drafts" | "templates" }
	| { action: "get_draft"; id: string }
	| { action: "create_draft"; name: string; definition: NeutralDefinition; editor?: EditorMetadata }
	| {
			action: "update_draft";
			id: string;
			expected_revision: number;
			name: string;
			definition: NeutralDefinition;
			editor?: EditorMetadata;
	  }
	| { action: "validate"; definition: unknown }
	| {
			action: "preview";
			definition: NeutralDefinition;
			inputs: Record<string, JsonValue>;
			subject: Record<string, JsonValue> | null;
	  }
	| { action: "publish"; id: string; expected_revision: number }
	| { action: "get_publication"; id: string; revision: number }
	| { action: "clone_publication"; id: string; revision: number; name: string }
	| { action: "save_preset"; id: string; expected_revision: number; instance: ActivityInstance }
	| { action: "get_preset"; id: string; revision: number }
	| { action: "inspect"; run_id: string; after_sequence: number; limit: number };
export function neutralClient(project: () => string) {
	return async <K extends keyof NeutralReplies>(action: NeutralAction, expected: K): Promise<NeutralReplies[K]> => {
		const reply = await invoke<{ type: string; value: NeutralReplies[K] }>("workflow_neutral_action", {
			project: project(),
			action,
		});
		if (reply.type !== expected) throw new Error(`Unexpected neutral workflow reply: ${reply.type}`);
		return reply.value;
	};
}
export function blankDefinition(): NeutralDefinition {
	return {
		control: {
			nodes: { start: { type: "start" }, end: { type: "end", outcome: "completed" } },
			edges: { start: { next: "end" } },
		},
		inputs: { fields: {} },
		outputs: { fields: {} },
		outcomes: ["completed"],
		subject: null,
		instances: [],
		input_bindings: {},
		conditions: {},
		collections: {},
		loops: {},
		forks: {},
		output_bindings: {},
		subflows: [],
	};
}
