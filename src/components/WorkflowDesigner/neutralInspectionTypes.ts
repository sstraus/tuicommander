import type { JsonValue, ObjectSchema } from "./catalogTypes";
import type { Diagnostic } from "./neutralClient";

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
	snapshot: Record<string, JsonValue>;
	events: (Record<string, JsonValue> & { sequence: number })[];
}
