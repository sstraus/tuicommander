import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { expect, it } from "vitest";
import type { ObjectSchema } from "../../components/WorkflowDesigner/catalogTypes";
import { type InputBindings, MappingFields } from "../../components/WorkflowDesigner/MappingFields";

function mount(
	initial: InputBindings = {},
	schema: ObjectSchema = { fields: { payload: { value_type: "object", required: true, default: null } } },
) {
	let current = initial;
	const changes: InputBindings[] = [];
	render(() => {
		const [bindings, setBindings] = createSignal(initial);
		return (
			<MappingFields
				schema={schema}
				bindings={bindings()}
				onChange={(next) => {
					current = next;
					changes.push(next);
					setBindings(next);
				}}
			/>
		);
	});
	return { read: () => current, changes };
}
const select = (name: string, value: string) =>
	fireEvent.change(screen.getByRole("combobox", { name }), { target: { value } });
const input = (name: string, value: string) =>
	fireEvent.input(screen.getByRole("textbox", { name }), { target: { value } });
const click = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

it("catches adding or removing an object member manufacturing null or clobbering siblings", () => {
	const view = mount({ unrelated: { source: "constant", value: false } });
	select("payload source", "object");
	expect(view.read()).toEqual({
		unrelated: { source: "constant", value: false },
		payload: { source: "object", fields: {} },
	});
	input("payload new member name", "first");
	click("Add payload member");
	expect(view.read().payload).toEqual({ source: "object", fields: { first: { source: "workflow_input", field: "" } } });
	input('payload member "first" source field', "request");
	input("payload new member name", "second");
	click("Add payload member");
	select('payload member "second" source', "object");
	input('payload member "second" new member name', "child");
	click('Add payload member "second" member');
	select('payload member "second" member "child" source', "projection");
	select('payload member "second" member "child" base source', "workflow_input");
	input('payload member "second" member "child" base source field', "data");
	input('payload member "second" member "child" JSON pointer', "/a//b~1c/~0/");
	select('payload member "second" member "child" result type', "boolean");
	expect(view.read()).toEqual({
		unrelated: { source: "constant", value: false },
		payload: {
			source: "object",
			fields: {
				first: { source: "workflow_input", field: "request" },
				second: {
					source: "object",
					fields: {
						child: {
							source: "projection",
							from: { source: "workflow_input", field: "data" },
							pointer: "/a//b~1c/~0/",
							value_type: "boolean",
						},
					},
				},
			},
		},
	});
	input('payload member "first" name', "renamed");
	click('Rename payload member "first"');
	click('Remove payload member "second"');
	expect(view.read()).toEqual({
		unrelated: { source: "constant", value: false },
		payload: { source: "object", fields: { renamed: { source: "workflow_input", field: "request" } } },
	});
	click('Remove payload member "renamed"');
	expect(view.read().payload).toEqual({ source: "object", fields: {} });
});

it("catches duplicate renames overwriting an existing member", () => {
	const initial: InputBindings = {
		payload: {
			source: "object",
			fields: { one: { source: "constant", value: true }, two: { source: "constant", value: false } },
		},
	};
	const view = mount(initial);
	input('payload member "one" name', "two");
	click('Rename payload member "one"');
	expect(screen.getByRole("alert")).toHaveTextContent("already exists");
	expect(view.changes).toEqual([]);
	input("payload new member name", "two");
	click("Add payload member");
	expect(view.read()).toEqual(initial);
});

it("catches projection edits replacing base identity or normalizing empty pointer segments", () => {
	const view = mount({
		payload: {
			source: "projection",
			from: { source: "scope_output", scope: "approved", field: "receipt" },
			pointer: "/",
			value_type: "object",
		},
	});
	input("payload JSON pointer", "/items//~1/~0/");
	select("payload result type", "array");
	expect(view.read()).toEqual({
		payload: {
			source: "projection",
			from: { source: "scope_output", scope: "approved", field: "receipt" },
			pointer: "/items//~1/~0/",
			value_type: "array",
		},
	});
	input("payload base scope", "nested");
	select("payload base source", "activity_output");
	fireEvent.input(screen.getByRole("spinbutton", { name: "payload base activity ordinal" }), {
		target: { value: "7" },
	});
	input("payload base source field", "document");
	expect(view.read().payload).toEqual({
		source: "projection",
		from: { source: "activity_output", activity: 7, field: "document" },
		pointer: "/items//~1/~0/",
		value_type: "array",
	});
	const options = Array.from(
		(screen.getByRole("combobox", { name: "payload base source" }) as HTMLSelectElement).options,
	).map((option) => option.value);
	expect(options).toEqual(["workflow_input", "activity_output", "scope_output"]);
	input("payload JSON pointer", "");
	expect(view.read().payload).toMatchObject({ pointer: "" });
});

it("catches nested string constants coercing numeric-looking text and JSON constants losing types", () => {
	const view = mount({ payload: { source: "object", fields: { value: { source: "constant", value: "before" } } } });
	input('payload member "value"', "42");
	expect(view.read().payload).toEqual({ source: "object", fields: { value: { source: "constant", value: "42" } } });
	select('payload member "value" constant type', "boolean");
	select('payload member "value"', "false");
	expect(view.read().payload).toEqual({ source: "object", fields: { value: { source: "constant", value: false } } });
	select('payload member "value" constant type', "array");
	fireEvent.change(screen.getByRole("textbox", { name: 'payload member "value"' }), {
		target: { value: '[42,"42",false,null]' },
	});
	expect(view.read().payload).toEqual({
		source: "object",
		fields: { value: { source: "constant", value: [42, "42", false, null] } },
	});
	click('Use default / unset payload member "value"');
	expect(view.read().payload).toEqual({ source: "object", fields: { value: { source: "workflow_input", field: "" } } });
});

it("catches malformed imports disappearing or being repaired while editing a sibling", () => {
	const malformed = JSON.parse(
		'{"source":"object","fields":{"broken":{"source":"projection","from":{"source":"constant","value":{"a":1}},"pointer":"/a","value_type":"integer"},"unknown":{"source":"future","raw":42},"bad":{"source":"object","fields":null},"good":{"source":"workflow_input","field":"old"}}}',
	) as InputBindings[string];
	const view = mount({ payload: malformed });
	expect(screen.getByLabelText('payload member "broken" base imported source')).toHaveTextContent('"constant"');
	expect(screen.getByLabelText('payload member "unknown" imported source')).toHaveTextContent('"raw": 42');
	expect(screen.getByLabelText('payload member "bad" imported source')).toHaveTextContent('"fields": null');
	expect(view.changes).toEqual([]);
	input('payload member "good" source field', "new");
	expect(view.read()).toEqual({
		payload: {
			source: "object",
			fields: {
				broken: {
					source: "projection",
					from: { source: "constant", value: { a: 1 } },
					pointer: "/a",
					value_type: "integer",
				},
				unknown: { source: "future", raw: 42 },
				bad: { source: "object", fields: null },
				good: { source: "workflow_input", field: "new" },
			},
		},
	});
});

it("catches flat callers losing string constants or the unbound distinction", () => {
	const view = mount({}, { fields: { text: { value_type: "string", required: false, default: "" } } });
	select("text source", "constant");
	input("text", "42");
	expect(view.read()).toEqual({ text: { source: "constant", value: "42" } });
	select("text source", "");
	expect(view.read()).toEqual({});
});

it("catches a return mapping row remounting on every keystroke when the schema is read from the edited definition", () => {
	// WorkflowInterface passes `definition.outputs` while each edit replaces the definition.
	const outputs: ObjectSchema = { fields: { tests_passed: { value_type: "boolean", required: true, default: null } } };
	let current: InputBindings = {};
	render(() => {
		const [definition, setDefinition] = createSignal({ outputs, output_bindings: {} as InputBindings });
		return (
			<MappingFields
				schema={definition().outputs}
				bindings={definition().output_bindings}
				onChange={(output_bindings) => {
					current = output_bindings;
					setDefinition({ ...definition(), output_bindings });
				}}
			/>
		);
	});
	select("tests_passed source", "activity_output");
	const field = screen.getByRole("textbox", { name: "tests_passed source field" });
	fireEvent.input(field, { target: { value: "p" } });
	expect(screen.getByRole("textbox", { name: "tests_passed source field" })).toBe(field);
	fireEvent.input(field, { target: { value: "passed" } });
	fireEvent.input(screen.getByRole("spinbutton", { name: "tests_passed activity ordinal" }), {
		target: { value: "1" },
	});
	expect(current).toEqual({ tests_passed: { source: "activity_output", activity: 1, field: "passed" } });
});
