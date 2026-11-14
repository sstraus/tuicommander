/** Wire shapes from workflows/catalog.rs. Validation remains backend-owned. */
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type ValueType =
	| "workspace_reference"
	| "artifact_reference"
	| "credential_reference"
	| "null"
	| "string"
	| "boolean"
	| "integer"
	| "number"
	| "object"
	| "array";
export interface FieldSchema {
	value_type: ValueType;
	required: boolean;
	default: JsonValue;
}
export interface ObjectSchema {
	fields: Record<string, FieldSchema>;
}
export interface DescriptorId {
	namespace: string;
	name: string;
	version: number;
}
export interface ActivityDescriptor {
	id: DescriptorId;
	purpose: string;
	handler: { name: string; version: number };
	target: { type: "script"; invocation: JsonValue } | { type: "subflow"; publication: JsonValue } | null;
	config: ObjectSchema;
	inputs: ObjectSchema;
	subject: { required: boolean; fields: ObjectSchema } | null;
	outputs: ObjectSchema;
	outcomes: string[];
	required_capabilities: string[];
	execution_policies: string[];
}
export interface ActivityInstance {
	descriptor: DescriptorId;
	config: Record<string, JsonValue>;
	execution_policy: string;
}
