import type { JsonValue, ObjectSchema } from "./catalogTypes";
import type { PublicationRef } from "./neutralClient";

/** Exact serialized source-neutral contract from workflows/trigger.rs. */
export type TriggerSource = "github" | "gitlab" | "scheduled";
export interface TriggerEvent {
	source: TriggerSource;
	kind: string;
	resource: string;
	generation: string;
	event: string;
	delivery_id: string | null;
	schema: ObjectSchema;
	values: Record<string, JsonValue>;
	actor: string | null;
}
export interface TriggerMapping {
	inputs: Record<string, string>;
	subject: Record<string, string>;
}
export interface TriggerBindingDraft {
	id: string;
	publication: PublicationRef;
	source: TriggerSource;
	schedule?: { cron: string; timezone: string; text: string };
	kind: string;
	resource_prefix: string;
	events: string[];
	filters: Record<string, JsonValue>;
	ignored_actors: string[];
	mapping: TriggerMapping;
	limits: { max_activations: number; max_concurrency: number; max_duration_ms: number };
}
export interface NeutralTriggerBinding {
	draft: TriggerBindingDraft;
	revision: number;
	enabled: boolean;
}
export interface TriggerPreview {
	inputs: Record<string, JsonValue>;
	subject: Record<string, JsonValue> | null;
	overlaps: Record<string, number>;
}
export interface TriggerAdmission {
	binding_id: string;
	resource: string;
	generation: string;
	run_id: string | null;
	disposition: string;
}
