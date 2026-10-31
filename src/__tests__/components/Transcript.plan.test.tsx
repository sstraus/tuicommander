// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { acpTranscript } from "../../stores/acpTranscript";
import type { AcpStreamFrame } from "../../types/acp";
import emptyPlan from "../fixtures/ego-empty-plan-2558.json";

let sequence = 0;
function update(body: Record<string, unknown>) {
	acpTranscript.applyFrame({
		kind: "event",
		connectionId: "plan-connection",
		sessionId: "plan-session",
		generation: 1,
		sequence: ++sequence,
		turnId: null,
		event: { kind: "sessionUpdate", update: body },
	} as AcpStreamFrame);
}
function mount() {
	return render(() => (
		<Transcript
			entries={() => acpTranscript.entries("plan-session")}
			busy={() => false}
			emptyMessage="Empty"
			observeToolDuration={false}
		/>
	));
}
beforeEach(() => {
	acpTranscript.reset();
	sequence = 0;
});
afterEach(cleanup);
describe("ACP plan snapshots at the transcript boundary", () => {
	it("renders no card for the recorded new-session empty plan — catches blank Plan cards on every new turn", () => {
		update(emptyPlan);
		const view = mount();
		expect(view.queryByText("Plan")).toBeNull();
	});
	it("clears old steps when the full plan becomes empty — catches a stale plan surviving a reset", () => {
		update({ sessionUpdate: "plan", entries: [{ content: "Inspect the queue", status: "pending", priority: "high" }] });
		const view = mount();
		expect(view.getByText("Inspect the queue")).toBeTruthy();
		update(emptyPlan);
		expect(view.queryByText("Plan")).toBeNull();
		expect(view.queryByText("Inspect the queue")).toBeNull();
	});
	it("renders steps after an empty snapshot — catches suppressing later nonempty plans", () => {
		update(emptyPlan);
		const view = mount();
		update({
			sessionUpdate: "plan",
			entries: [{ content: "Verify the result", status: "in_progress", priority: "high" }],
		});
		expect(view.getByText("Plan")).toBeTruthy();
		expect(view.getByText("Verify the result")).toBeTruthy();
	});
});
