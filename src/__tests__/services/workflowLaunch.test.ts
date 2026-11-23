import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));

import { invoke } from "../../invoke";
import { listLaunchableWorkflows, startWorkflow } from "../../services/workflowLaunch";
import { workflowsUi } from "../../stores/workflowsUi";

type Action = { action: string; [key: string]: unknown };
const field = (required: boolean, value: unknown = null) => ({ value_type: "string", required, default: value });
const definition = (inputs: Record<string, unknown>, subject: unknown = null) => ({
	inputs: { fields: inputs },
	subject,
});
const draft = (id: string, published: number) => ({ id, name: `Flow ${id}`, latest_published_revision: published });

/** Answers each neutral action like the backend; records every action it received. */
function backend(publication: unknown) {
	const actions: Action[] = [];
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		const action = (args as { action: Action }).action;
		actions.push(action);
		if (action.action === "list_drafts") return { type: "drafts", value: [draft("a", 2), draft("b", 0)] };
		if (action.action === "get_publication") return { type: "publication", value: { definition: publication } };
		if (action.action === "start") return { type: "inspection", value: { snapshot: {}, events: [] } };
		throw new Error(`unexpected ${action.action}`);
	});
	return actions;
}

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	workflowsUi.close();
});

describe("workflow launch from a panel", () => {
	it("lists only workflows with a published revision", async () => {
		// catches: offering a draft that `start` refuses because no publication exists
		backend(definition({}));
		expect(await listLaunchableWorkflows("/repo")).toEqual([{ id: "a", name: "Flow a", revision: 2 }]);
	});

	it("starts the published revision with input defaults and the panel item as subject", async () => {
		// catches: starting the draft revision, or dropping the issue the user launched it from
		const actions = backend(definition({ mode: field(true, "fast"), note: field(false) }, { required: true }));
		await startWorkflow("/repo", { id: "a", name: "Flow a", revision: 2 }, { kind: "issue", number: 7 });
		const start = actions.find((a) => a.action === "start") as { request: Record<string, unknown> } | undefined;
		expect(start?.request).toMatchObject({
			publication: { project: "/repo", id: "a", revision: 2 },
			inputs: { mode: "fast" },
			subject: { kind: "issue", number: 7 },
		});
	});

	it("sends no subject to a workflow that declares none", async () => {
		// catches: the backend rejecting an unexpected subject
		const actions = backend(definition({}));
		await startWorkflow("/repo", { id: "a", name: "Flow a", revision: 2 }, { kind: "issue", number: 7 });
		expect(actions.find((a) => a.action === "start")).toMatchObject({ request: { subject: null } });
	});

	it("opens the Workflows space instead of starting when a required input has no default", async () => {
		// catches: a run started with a guessed value for an input only the user can give
		const actions = backend(definition({ target: field(true) }));
		await startWorkflow("/repo", { id: "a", name: "Flow a", revision: 2 }, {});
		expect(actions.some((a) => a.action === "start")).toBe(false);
		expect(workflowsUi.visible()).toBe(true);
		expect(workflowsUi.project()).toBe("/repo");
	});
});
