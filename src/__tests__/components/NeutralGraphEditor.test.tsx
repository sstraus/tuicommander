import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { expect, it, vi } from "vitest";
import type { ActivityDescriptor } from "../../components/WorkflowDesigner/catalogTypes";
import { NeutralGraphEditor } from "../../components/WorkflowDesigner/NeutralGraphEditor";
import { blankDefinition, type NeutralDefinition } from "../../components/WorkflowDesigner/neutralClient";
import recorded from "../fixtures/workflow/neutral-transport-authoring.json";

function mount(initial = blankDefinition()) {
	const [definition, setDefinition] = createSignal(initial);
	const [selected, setSelected] = createSignal("start");
	const [documentKey, setDocumentKey] = createSignal(0);
	const changed = vi.fn((next: NeutralDefinition) => setDefinition(next));
	render(() => (
		<NeutralGraphEditor
			definition={definition()}
			activities={recorded.catalog.value.activities as ActivityDescriptor[]}
			labels={{ start: "Begin", end: "Finish", work: "Review" }}
			selected={selected()}
			onSelect={setSelected}
			onChange={changed}
			documentKey={documentKey()}
		/>
	));
	return { definition, changed, setDocumentKey };
}

it("reconnects only the armed route, retaining sibling outcomes and the draft's non-control data", () => {
	const initial = structuredClone(recorded.draft.value.definition) as NeutralDefinition;
	// This is a user-authored condition, not an external-system fixture.
	initial.control.nodes.start = { type: "condition", condition: "ready" };
	initial.control.edges.start = { true: "work", false: "end", unknown: "work" };
	const state = mount(initial);
	fireEvent.click(screen.getByRole("button", { name: "Connect Begin true" }));
	fireEvent.click(screen.getByRole("button", { name: "Finish end" }));
	expect(state.definition()).toEqual({
		...initial,
		control: {
			...initial.control,
			edges: { ...initial.control.edges, start: { true: "end", false: "end", unknown: "work" } },
		},
	});
	expect(screen.queryByRole("status")).not.toBeInTheDocument();
	expect(within(screen.getByLabelText("Named control routes")).getByText("true")).toBeInTheDocument();
});

it("selection, zoom, fit and cancellation cannot mutate or create routes", () => {
	const state = mount();
	fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
	fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
	fireEvent.click(screen.getByRole("button", { name: "Fit graph" }));
	fireEvent.click(screen.getByRole("button", { name: "Connect Begin next" }));
	fireEvent.keyDown(screen.getByLabelText("Scrollable workflow canvas"), { key: "Escape" });
	fireEvent.click(screen.getByRole("button", { name: "Finish end" }));
	expect(state.changed).not.toHaveBeenCalled();
	expect(state.definition().control.edges).toEqual({ start: { next: "end" } });
});

it("disarms a connection when opening another document with the same start and end IDs", () => {
	const state = mount();
	fireEvent.click(screen.getByRole("button", { name: "Connect Begin next" }));
	state.setDocumentKey(1);
	fireEvent.click(screen.getByRole("button", { name: "Finish end" }));
	expect(state.changed).not.toHaveBeenCalled();
	expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

it("keeps cyclic, disconnected and missing-target imports inspectable instead of crashing or hiding nodes", () => {
	const definition = blankDefinition();
	definition.control.nodes.orphan = { type: "wait", duration_ms: 1000 };
	definition.control.edges = { start: { next: "start", unsupported: "missing" } };
	const state = mount(definition);
	expect(screen.getByRole("button", { name: "Begin start" })).toBeInTheDocument();
	expect(screen.getByRole("button", { name: "Finish end" })).toBeInTheDocument();
	expect(screen.getByRole("button", { name: "Wait 1 wait" })).toBeInTheDocument();
	expect(screen.getByText(/Some routes reference missing nodes/)).toBeInTheDocument();
	expect(state.changed).not.toHaveBeenCalled();
});

it("keeps measuring the control canvas after a scope view replaces it", () => {
	const observed: Element[] = [];
	vi.stubGlobal(
		"ResizeObserver",
		class {
			observe(el: Element) {
				observed.push(el);
			}
			disconnect() {}
			unobserve() {}
		},
	);
	try {
		const initial = blankDefinition();
		initial.control.nodes.loop_1 = {
			type: "loop",
			scope: "loop_1",
			condition: "c",
		} as NeutralDefinition["control"]["nodes"][string];
		render(() => (
			<NeutralGraphEditor
				definition={initial}
				activities={[]}
				labels={{}}
				selected="loop_1"
				onSelect={() => {}}
				onChange={() => {}}
				onInspectBody={() => {}}
			/>
		));
		fireEvent.click(screen.getByRole("button", { name: /Open scope/ }));
		fireEvent.click(screen.getByRole("button", { name: "Workflow graph" }));
		const canvas = screen.getByLabelText("Scrollable workflow canvas");
		expect(observed).toContain(canvas);
	} finally {
		vi.unstubAllGlobals();
	}
});
