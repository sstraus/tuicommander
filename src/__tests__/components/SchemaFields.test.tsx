import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeAll, expect, it, vi } from "vitest";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce([]);
	await activityStore.hydrate();
});

import type { JsonValue } from "../../components/WorkflowDesigner/catalogTypes";
import { type InputBindings, MappingFields } from "../../components/WorkflowDesigner/MappingFields";
import { SchemaFields } from "../../components/WorkflowDesigner/SchemaFields";

it("preserves false and empty string while unsetting removes the binding instead of writing null", () => {
	let current: Record<string, JsonValue> = {};
	render(() => {
		const [values, setValues] = createSignal<Record<string, JsonValue>>({});
		return (
			<SchemaFields
				schema={{
					fields: {
						enabled: { value_type: "boolean", required: false, default: true },
						text: { value_type: "string", required: true, default: "fallback" },
					},
				}}
				values={values()}
				onChange={(next) => {
					current = next;
					setValues(next);
				}}
			/>
		);
	});
	fireEvent.change(screen.getByRole("combobox", { name: "enabled" }), { target: { value: "false" } });
	fireEvent.input(screen.getByRole("textbox", { name: "text" }), { target: { value: "" } });
	expect(current).toEqual({ enabled: false, text: "" });
	fireEvent.click(screen.getByRole("button", { name: "Use default / unset enabled" }));
	expect(current).toEqual({ text: "" });
	expect(screen.getByRole("combobox", { name: "enabled" })).toHaveValue("true");
});

it("keeps invalid JSON local rather than replacing the last valid structured value", () => {
	let current: Record<string, JsonValue> = { payload: { value: 1 } };
	render(() => (
		<SchemaFields
			schema={{ fields: { payload: { value_type: "object", required: true, default: null } } }}
			values={current}
			onChange={(next) => {
				current = next;
			}}
		/>
	));
	fireEvent.change(screen.getByRole("textbox", { name: "payload" }), { target: { value: "{" } });
	expect(screen.getByRole("alert")).toHaveTextContent("valid JSON");
	expect(current).toEqual({ payload: { value: 1 } });
});

it("distinguishes an explicit null mapping from an unbound optional input", () => {
	let current: InputBindings = {};
	render(() => {
		const [bindings, setBindings] = createSignal<InputBindings>({});
		return (
			<MappingFields
				schema={{ fields: { result: { value_type: "object", required: false, default: null } } }}
				bindings={bindings()}
				onChange={(next) => {
					current = next;
					setBindings(next);
				}}
			/>
		);
	});
	fireEvent.change(screen.getByRole("combobox", { name: "result source" }), { target: { value: "constant" } });
	expect(current).toEqual({ result: { source: "constant", value: null } });
	fireEvent.change(screen.getByRole("combobox", { name: "result source" }), { target: { value: "" } });
	expect(current).toEqual({});
});

// Catches: the Designer dropping a nested projection or object source, or
// accepting a composite edit that silently changes the source kind.
it("keeps projection and object sources as backend binding JSON", () => {
	let current: InputBindings = {};
	render(() => {
		const [bindings, setBindings] = createSignal<InputBindings>({});
		return (
			<MappingFields
				schema={{ fields: { outcome: { value_type: "string", required: true, default: null } } }}
				bindings={bindings()}
				onChange={(next) => {
					current = next;
					setBindings(next);
				}}
			/>
		);
	});
	fireEvent.change(screen.getByRole("combobox", { name: "outcome source" }), { target: { value: "projection" } });
	expect(current).toEqual({
		outcome: {
			source: "projection",
			from: { source: "activity_output", activity: 0, field: "" },
			pointer: "/",
			value_type: "string",
		},
	});
	const projection = {
		source: "projection",
		from: { source: "activity_output", activity: 1, field: "receipt" },
		pointer: "/assessment/decision/outcome",
		value_type: "string",
	};
	const editor = screen.getByRole("textbox", { name: "outcome composite source" });
	fireEvent.change(editor, { target: { value: JSON.stringify(projection) } });
	expect(current).toEqual({ outcome: projection });
	fireEvent.change(editor, { target: { value: JSON.stringify({ source: "constant", value: "x" }) } });
	expect(screen.getByRole("alert")).toHaveTextContent("projection source");
	expect(current).toEqual({ outcome: projection });
	fireEvent.change(screen.getByRole("combobox", { name: "outcome source" }), { target: { value: "object" } });
	expect(current).toEqual({ outcome: { source: "object", fields: {} } });
});
