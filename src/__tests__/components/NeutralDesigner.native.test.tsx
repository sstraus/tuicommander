import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterAll, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { CorrectionRegionEditor, RetainedCorrections } from "../../components/WorkflowDesigner/CorrectionRegion";
import { NeutralDesigner } from "../../components/WorkflowDesigner/NeutralDesigner";
import { NeutralInspection } from "../../components/WorkflowDesigner/NeutralInspection";
import type { NeutralDefinition } from "../../components/WorkflowDesigner/neutralClient";
import { useNeutralDraft } from "../../components/WorkflowDesigner/useNeutralDraft";
import { activityStore } from "../../stores/activityStore";
import captured from "../fixtures/workflow/neutral-native-catalog.json";

// Direct-service production capture, not a live server. Keep the real RPC stack.
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async () => () => {}),
}));
const requests: Record<string, unknown>[] = [];
beforeAll(async () => {
	vi.stubGlobal("__TAURI_SHIM__", true);
	vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
		if (init?.method === "GET") return Response.json([]);
		const url = new URL(String(input));
		expect(url.pathname).toBe("/workflows/neutral/action");
		expect(url.searchParams.get("path")).toBe(captured.project);
		const action = JSON.parse(String(init?.body)) as Record<string, unknown>;
		requests.push(action);
		switch (action.action) {
			case "catalog":
				return Response.json(captured.catalog);
			case "list_drafts":
				return Response.json({ type: "drafts", value: [] });
			case "preview":
				expect(action).toEqual({
					action: "preview",
					definition: captured.preparation.definition,
					inputs: captured.preparation.inputs,
					subject: captured.preparation.subject,
				});
				return Response.json(captured.preparation.reply);
			case "validate":
				if (JSON.stringify(action.definition) === JSON.stringify(captured.subflow.valid.definition))
					return Response.json(captured.subflow.valid.reply);
				expect(action.definition).toEqual(captured.subflow.invalid.definition);
				return Response.json(captured.subflow.invalid.reply);
			case "correction_region":
				expect(action.region).toEqual(captured.correction_region.reply.value.region);
				return Response.json(captured.correction_region.reply);
			case "get_publication":
				expect(action).toEqual({
					action: "get_publication",
					id: captured.correction_region.selected_publication.value.id,
					revision: captured.correction_region.selected_publication.value.revision,
				});
				return Response.json(captured.correction_region.selected_publication);
			default:
				throw new Error(`No captured response for ${action.action}`);
		}
	});
	await activityStore.hydrate();
});
afterAll(() => vi.unstubAllGlobals());
beforeEach(() => {
	requests.length = 0;
});

it("renders every installed family from the actual composite catalog with generic native policy and ports", async () => {
	render(() => <NeutralDesigner project={captured.project} />);
	for (const descriptor of captured.catalog.value.activities) {
		expect(
			await screen.findByRole("button", { name: `Add ${descriptor.id.name} · v${descriptor.id.version}` }),
		).toBeInTheDocument();
	}
	fireEvent.click(screen.getByRole("button", { name: "Add commit.create · v1" }));
	const inspector = screen.getByRole("complementary", { name: "Neutral node inspector" });
	expect(
		within(inspector).getByText("Commit the exact approved index through the native Git service"),
	).toBeInTheDocument();
	expect(within(inspector).getByRole("combobox", { name: "Execution policy" })).toHaveTextContent("native");
	expect(within(inspector).getByRole("combobox", { name: "request source" })).toBeInTheDocument();
	expect(requests.every((request) => ["catalog", "list_drafts", "templates"].includes(String(request.action)))).toBe(
		true,
	);
});

it("renders real binding-free native preparation and approval intent without dispatching or persisting", async () => {
	let sampleState: ReturnType<typeof useNeutralDraft> | undefined;
	function Sample() {
		const state = useNeutralDraft(() => captured.project);
		sampleState = state;
		return <NeutralInspection state={state} project={captured.project} />;
	}
	render(() => <Sample />);
	await waitFor(() => expect(sampleState?.busy()).toBe(false));
	// Seed the user-authored sample after initial metadata loading.
	sampleState?.edit(captured.preparation.definition as NeutralDefinition);
	fireEvent.input(screen.getByLabelText("Sample inputs (JSON)"), {
		target: { value: JSON.stringify(captured.preparation.inputs) },
	});
	fireEvent.input(screen.getByLabelText("Sample subject (JSON or null)"), {
		target: { value: JSON.stringify(captured.preparation.subject) },
	});
	fireEvent.click(screen.getByRole("button", { name: "Preview sample IO and effect intents" }));
	const result = await screen.findByRole("region", { name: "Resolved preview" });
	const blocks = within(result).getAllByText((_, node) => node?.tagName === "PRE");
	expect(JSON.parse(blocks[0].textContent ?? "null")).toEqual(captured.preparation.reply.value.activities[0].inputs);
	expect(JSON.parse(blocks[1].textContent ?? "null")).toEqual(
		captured.preparation.reply.value.activities[0].effect_intent,
	);
	expect(
		requests.every((request) => ["catalog", "list_drafts", "templates", "preview"].includes(String(request.action))),
	).toBe(true);
});

it("applies the actual admitted subflow and rejects its incompatible typed mapping before replacing the graph", async () => {
	render(() => <NeutralDesigner project={captured.project} />);
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
	const editor = screen.getByLabelText("Neutral definition JSON");
	fireEvent.change(editor, { target: { value: JSON.stringify(captured.subflow.valid.definition) } });
	await waitFor(() =>
		expect(JSON.parse((editor as HTMLTextAreaElement).value)).toEqual(captured.subflow.valid.definition),
	);
	await waitFor(() => expect(screen.getByRole("button", { name: "Save neutral draft" })).not.toBeDisabled());
	fireEvent.change(editor, { target: { value: JSON.stringify(captured.subflow.invalid.definition) } });
	expect(await screen.findByRole("alert")).toHaveTextContent("binding.value: incompatible constant");
	await waitFor(() => expect(screen.getByRole("button", { name: "Validate links and inputs" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Validate links and inputs" }));
	await waitFor(() => expect(requests.filter((request) => request.action === "validate")).toHaveLength(3));
	expect(requests.filter((request) => request.action === "validate").map((request) => request.definition)).toEqual([
		captured.subflow.valid.definition,
		captured.subflow.invalid.definition,
		captured.subflow.valid.definition,
	]);
	expect(screen.getByRole("button", { name: "Publish neutral draft" })).toBeDisabled();
});

it("renders the real correction contract and exact selected child without publication or mutable correction counters", async () => {
	function Sample() {
		const state = useNeutralDraft(() => captured.project);
		return <CorrectionRegionEditor state={state} />;
	}
	render(() => <Sample />);
	screen.getByText("Bounded correction region").closest("details")?.setAttribute("open", "");
	await waitFor(() => expect(screen.getByLabelText("Correction region (JSON)")).not.toBeDisabled());
	fireEvent.input(screen.getByLabelText("Correction region (JSON)"), {
		target: { value: JSON.stringify(captured.correction_region.reply.value.region) },
	});
	fireEvent.click(screen.getByRole("button", { name: "Inspect correction region" }));
	const region = await screen.findByRole("region", { name: "Correction region contract" });
	expect(within(region).getByText("independent_review")).toBeInTheDocument();
	expect(within(region).getByText("Maximum corrections: 2")).toBeInTheDocument();
	const definition = within(region)
		.getAllByText((_, node) => node?.tagName === "PRE")
		.at(-1);
	expect(JSON.parse(definition?.textContent ?? "null")).toEqual(captured.correction_region.reply.value.definition);
	fireEvent.click(
		within(region).getByRole("button", { name: "Inspect selected child selected-correction · revision 1" }),
	);
	const child = await screen.findByRole("region", { name: "Selected correction child publication" });
	expect(JSON.parse(within(child).getByText((_, node) => node?.tagName === "PRE").textContent ?? "null")).toEqual(
		captured.correction_region.selected_publication.value,
	);
	expect(
		requests.every((request) =>
			["catalog", "list_drafts", "templates", "correction_region", "get_publication"].includes(String(request.action)),
		),
	).toBe(true);
	const inspection = captured.correction_region.inspection;
	const [scope, setScope] = createSignal(inspection.active_scope);
	render(() => <RetainedCorrections scope={scope()} />);
	expect(screen.getByRole("region", { name: "Retained correction budget" })).toHaveTextContent(
		"Consumed corrections: 0",
	);
	// A later retained iteration, not a UI counter, drives the displayed budget.
	setScope({ ...inspection.active_scope, kind: { type: "loop", iteration: 3 } });
	expect(screen.getByRole("region", { name: "Retained correction budget" })).toHaveTextContent(
		"Consumed corrections: 2",
	);
});
