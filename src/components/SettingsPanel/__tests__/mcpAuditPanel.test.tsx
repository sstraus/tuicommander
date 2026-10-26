import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { rpc, mockInvoke } = vi.hoisted(() => ({ rpc: vi.fn(), mockInvoke: vi.fn() }));
vi.mock("../../../transport", () => ({ rpc }));
vi.mock("../../../invoke", () => ({ invoke: mockInvoke, listen: vi.fn().mockResolvedValue(vi.fn()) }));
vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { LocalMcpPanel } from "../tabs/services/LocalMcpPanel";
import { McpAuditPanel } from "../tabs/services/McpAuditPanel";

const row = (over: Record<string, unknown> = {}) => ({
	call_id: "11111111-1111-4111-8111-111111111111",
	client_call_id: null,
	parent_operation_id: null,
	operation: "session.list",
	target_digest: "abcdef0123456789".repeat(4),
	started_ms: 1_760_000_000_000,
	response_text_bytes: 321,
	application_error: false,
	transport_error: "unobserved_after_reply_construction",
	effect: "completed",
	outcome: "replied",
	...over,
});

const projection = (records: unknown[]) => ({ enabled: true, records, retained: records.length, evicted: 0 });

describe("McpAuditPanel", () => {
	beforeEach(() => rpc.mockReset());
	afterEach(cleanup);

	// Catches: a long opaque operation name widening the table with no way to read it in full.
	it("clips a 64-hex operation and keeps the full name in a title tooltip", async () => {
		const operation = "ab".repeat(32);
		rpc.mockResolvedValue(projection([row({ operation })]));
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelector("tbody tr")).toBeTruthy());
		const cell = Array.from(container.querySelectorAll("tbody td")).find((td) => td.textContent === operation);
		expect(cell?.getAttribute("title")).toBe(operation);
		expect(cell?.className).toMatch(/clip/);
	});

	// Catches: a clipped time, bytes or badge cell with no way to read its full text.
	it("gives every clipped cell a title with its full text", async () => {
		rpc.mockResolvedValue(projection([row({ outcome: "cancelled", effect: "not_started" })]));
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelector("tbody tr")).toBeTruthy());
		const cells = Array.from(container.querySelectorAll("tbody tr td"));
		expect(cells).toHaveLength(6);
		for (const td of cells) expect(td.getAttribute("title")).toBeTruthy();
		for (const i of [0, 3, 4]) expect(cells[i].getAttribute("title")).toBe(cells[i].textContent);
		expect(cells[5].getAttribute("title")).toContain("before the operation started");
	});

	// Catches: a row rendering response bodies or secrets instead of the body-free whitelist.
	it("renders only the body-free columns even when a row carries extra fields", async () => {
		rpc.mockResolvedValue(
			projection([row({ response_body: "SECRET_BODY", arguments: { token: "SECRET_TOKEN" }, error: "SECRET_ERR" })]),
		);
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(1));
		expect(container.textContent).not.toContain("SECRET");
		expect(container.textContent).toContain("session.list");
		expect(container.textContent).toContain("321");
		expect(container.textContent).toContain("abcdef01");
		expect(container.textContent).not.toContain("abcdef0123456789abcdef0123456789");
	});

	// Catches: an unknown effect (or a reply) being shown as success.
	it("labels unknown effect as unknown and a cancelled call as cancelled, never as success", async () => {
		rpc.mockResolvedValue(
			projection([
				row({ effect: "unknown", outcome: "cancelled", response_text_bytes: null, application_error: null }),
			]),
		);
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(1));
		const text = container.querySelector("tbody tr")?.textContent ?? "";
		expect(text).toContain("Cancelled");
		expect(text).toContain("Unknown");
		expect(text).not.toMatch(/success|completed|ok/i);
	});

	// Catches: resolved identities discarded in favor of opaque digests, and known read-only effects mislabeled.
	it("shows a known target name and id with classified effects and an uncertainty explanation", async () => {
		rpc.mockResolvedValue(
			projection([
				row({
					target: { kind: "session", id: "12345678-1234-4234-8234-123456789abc", name: "Build worker" },
					effect: "read_only",
				}),
				row({ effect: "unknown", outcome: "cancelled" }),
			]),
		);
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelectorAll("tbody tr")).toHaveLength(2));
		const rows = container.querySelectorAll("tbody tr");
		expect(rows[0].textContent).toContain("Build worker · 12345678");
		expect(rows[0].textContent).toContain("Read-only");
		expect(rows[0].querySelectorAll("td")[2].title).toContain("12345678-1234-4234-8234-123456789abc");
		expect(rows[1].querySelectorAll("td")[5].title).toContain("Completion is uncertain");
	});

	// Catches: the outcome filter hiding nothing, or hiding the wrong rows.
	it("filters rows by outcome", async () => {
		rpc.mockResolvedValue(
			projection([
				row({ operation: "repo.list", outcome: "replied" }),
				row({ operation: "progress", outcome: "error", application_error: true }),
			]),
		);
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(2));
		const select = container.querySelector("select") as HTMLSelectElement;
		fireEvent.change(select, { target: { value: "error" } });
		await waitFor(() => expect(container.querySelectorAll("tbody tr").length).toBe(1));
		expect(container.querySelector("tbody tr")?.textContent).toContain("progress");
	});

	// Catches: stale rows or no refresh — one fetch on open, one more per Refresh click, none otherwise.
	it("fetches once on open and again only on demand", async () => {
		vi.useFakeTimers();
		try {
			rpc.mockResolvedValue(projection([row()]));
			const { getByText } = render(() => <McpAuditPanel />);
			await vi.advanceTimersByTimeAsync(0);
			expect(rpc).toHaveBeenCalledTimes(1);
			expect(rpc).toHaveBeenCalledWith("get_mcp_audit");
			await vi.advanceTimersByTimeAsync(120_000);
			expect(rpc).toHaveBeenCalledTimes(1);
			fireEvent.click(getByText("Refresh"));
			await vi.advanceTimersByTimeAsync(0);
			expect(rpc).toHaveBeenCalledTimes(2);
		} finally {
			vi.useRealTimers();
		}
	});

	// Catches: a disabled backend rendering an empty table that looks like "nothing happened".
	it("shows the disabled state, not an empty table, when the backend reports disabled", async () => {
		rpc.mockResolvedValue({ enabled: false, records: [] });
		const { container } = render(() => <McpAuditPanel />);
		await waitFor(() => expect(container.textContent).toContain("disabled"));
		expect(container.querySelector("table")).toBeNull();
	});
});

describe("LocalMcpPanel audit toggle", () => {
	afterEach(cleanup);

	// Catches: the view mounting and fetching before the toggle is persisted, so a just-enabled audit reads "disabled".
	it("shows the enabled, empty view after the toggle is switched on, not a stale disabled notice", async () => {
		let saved = false;
		const config = () => ({
			mcp_capabilities: { denied: [], migration_unknown_tools: [] },
			mcp_call_audit_enabled: saved,
		});
		rpc.mockReset();
		mockInvoke.mockReset();
		rpc.mockImplementation(async (cmd: string) => {
			if (cmd === "load_config") return config();
			if (cmd === "get_mcp_audit") return saved ? projection([]) : { enabled: false, records: [] };
			throw new Error(`unmocked ${cmd}`);
		});
		mockInvoke.mockImplementation(async (cmd: string) => {
			if (cmd === "load_config") return config();
			if (cmd === "save_config") {
				await new Promise((resolve) => setTimeout(resolve, 30));
				saved = true;
			}
			return undefined;
		});
		const { container } = render(() => <LocalMcpPanel />);
		const toggle = await waitFor(() => {
			const el = container.querySelector<HTMLInputElement>('[data-setting-id="app.mcp_call_audit_enabled"]');
			expect(el).not.toBeNull();
			return el as HTMLInputElement;
		});
		await new Promise((resolve) => setTimeout(resolve, 20)); // let the initial load_config apply land
		toggle.checked = true;
		fireEvent.change(toggle);
		await waitFor(() => expect(saved).toBe(true));
		await new Promise((resolve) => setTimeout(resolve, 50));
		expect(container.textContent).toContain("No calls recorded yet.");
		expect(container.textContent).not.toContain("The call audit is disabled.");
	});
});
