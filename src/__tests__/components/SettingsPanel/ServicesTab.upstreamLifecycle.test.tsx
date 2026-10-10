import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));

import { UpstreamMcpPanel } from "../../../components/SettingsPanel/tabs/services/UpstreamMcpPanel";
import { rpc } from "../../../transport";

describe("UpstreamMcpPanel lifecycle", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "load_mcp_upstreams") return Promise.resolve({ servers: [] });
			if (command === "get_mcp_upstream_status") return Promise.resolve({ upstreams: [] });
			return Promise.resolve(undefined);
		});
	});

	afterEach(() => {
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.clearAllMocks();
	});

	it("owns the existing three-second status poll and clears it on unmount", async () => {
		const view = render(() => <UpstreamMcpPanel />);
		const statusCalls = () =>
			vi.mocked(rpc).mock.calls.filter(([command]) => command === "get_mcp_upstream_status").length;

		// The third argument is the machine the panel is editing — `undefined` is
		// this one, which is where the panel opens.
		expect(rpc).toHaveBeenCalledWith("load_mcp_upstreams", {}, undefined);
		expect(statusCalls()).toBe(1);

		await vi.advanceTimersByTimeAsync(3000);
		expect(statusCalls()).toBe(2);

		view.unmount();
		await vi.advanceTimersByTimeAsync(6000);
		expect(statusCalls()).toBe(2);
	});

	it("keeps Bearer first and adds several removable masked header rows", async () => {
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Add upstream server").click();
		expect((view.getByDisplayValue("Bearer token") as HTMLSelectElement).value).toBe("bearer");
		const bearer = view.getByPlaceholderText("API key for remote MCP servers (optional, stored in OS keychain)");
		expect(bearer.getAttribute("type")).toBe("password");
		view.getByTitle("Add secret header").click();
		view.getByTitle("Add secret header").click();
		expect(view.getByLabelText("Header secret 1").getAttribute("type")).toBe("password");
		expect(view.getByLabelText("Header secret 2").getAttribute("type")).toBe("password");
		expect(
			bearer.compareDocumentPosition(view.getByLabelText("Header name 1")) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		view.getByTitle("Remove header 1").click();
		expect(view.queryByLabelText("Header name 2")).toBeNull();
		view.unmount();
	});

	it("sends header secrets only to the credential RPC and never config or logs", async () => {
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Add upstream server").click();
		fireEvent.input(view.getByPlaceholderText("Name (e.g. context7, github)"), { target: { value: "example" } });
		fireEvent.input(view.getByPlaceholderText("URL (e.g. http://localhost:8080/mcp)"), {
			target: { value: "https://example.com/mcp" },
		});
		view.getByTitle("Add secret header").click();
		fireEvent.input(view.getByLabelText("Header name 1"), { target: { value: "x-api-key" } });
		fireEvent.input(view.getByLabelText("Header secret 1"), { target: { value: "DUMMY_UI_SENTINEL" } });
		view.getByText("Add").click();
		await vi.advanceTimersByTimeAsync(0);
		const credentials = vi.mocked(rpc).mock.calls.filter(([c]) => c === "save_mcp_upstream_credential");
		expect(credentials).toHaveLength(1);
		expect(credentials[0][1]).toMatchObject({
			name: "example",
			url: "https://example.com/mcp",
			token: "DUMMY_UI_SENTINEL",
			header: { name: "x-api-key" },
		});
		const saves = vi.mocked(rpc).mock.calls.filter(([c]) => c === "save_mcp_upstreams");
		expect(saves).toHaveLength(1);
		expect(JSON.stringify(saves)).not.toContain("DUMMY_UI_SENTINEL");
		const { appLogger } = await import("../../../stores/appLogger");
		expect(JSON.stringify(vi.mocked(appLogger.info).mock.calls)).not.toContain("DUMMY_UI_SENTINEL");
		expect(view.queryByLabelText("Header secret 1")).toBeNull();
		view.unmount();
	});

	it("keeps saved header secrets masked and removes their credential after edit", async () => {
		const header = { name: "x-api-key", credential_ref: "b1bc9b6e-b6e6-4ae7-89b8-e1d977c8fb11" };
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "load_mcp_upstreams")
				return Promise.resolve({
					servers: [
						{
							id: "saved",
							name: "example",
							transport: { type: "http", url: "https://example.com/mcp" },
							enabled: true,
							timeout_secs: 30,
							headers: [header],
						},
					],
				});
			if (command === "get_mcp_upstream_status") return Promise.resolve({ upstreams: [] });
			return Promise.resolve(undefined);
		});
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Edit").click();
		const input = view.getByLabelText("Header secret 1") as HTMLInputElement;
		expect(input.type).toBe("password");
		expect(input.value).toBe("");
		expect(input.placeholder).toBe("Saved secret (leave blank to keep)");
		view.getByText("Save").click();
		await vi.advanceTimersByTimeAsync(0);
		expect(vi.mocked(rpc).mock.calls.some(([c]) => c === "save_mcp_upstream_credential")).toBe(false);
		view.getByTitle("Edit").click();
		view.getByTitle("Remove header 1").click();
		view.getByText("Save").click();
		await vi.advanceTimersByTimeAsync(0);
		expect(rpc).toHaveBeenCalledWith("delete_mcp_upstream_credential", { name: "example", header }, undefined);
		view.unmount();
	});

	it("does not persist config or expose a credential endpoint error after secret save fails", async () => {
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Add upstream server").click();
		fireEvent.input(view.getByPlaceholderText("Name (e.g. context7, github)"), { target: { value: "example" } });
		fireEvent.input(view.getByPlaceholderText("URL (e.g. http://localhost:8080/mcp)"), {
			target: { value: "https://example.com/mcp" },
		});
		view.getByTitle("Add secret header").click();
		fireEvent.input(view.getByLabelText("Header name 1"), { target: { value: "x-api-key" } });
		fireEvent.input(view.getByLabelText("Header secret 1"), { target: { value: "DUMMY_ERROR_SENTINEL" } });
		vi.mocked(rpc).mockRejectedValueOnce(new Error("DUMMY_ERROR_SENTINEL"));
		view.getByText("Add").click();
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByText("Failed to save upstream credentials")).toBeTruthy();
		expect(view.container.textContent).not.toContain("DUMMY_ERROR_SENTINEL");
		expect(vi.mocked(rpc).mock.calls.some(([c]) => c === "save_mcp_upstreams")).toBe(false);
		view.unmount();
	});

	it.each(["", "x key"])("rejects invalid header name %j before saving credentials", async (name) => {
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Add upstream server").click();
		view.getByTitle("Add secret header").click();
		fireEvent.input(view.getByLabelText("Header name 1"), { target: { value: name } });
		fireEvent.input(view.getByLabelText("Header secret 1"), { target: { value: "DUMMY_INVALID_SENTINEL" } });
		view.getByText("Add").click();
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByText("Invalid or reserved custom header name")).toBeTruthy();
		expect(vi.mocked(rpc).mock.calls.some(([c]) => c === "save_mcp_upstream_credential")).toBe(false);
		view.unmount();
	});

	it("rejects pasted CRLF instead of silently turning it into a valid name", async () => {
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Add upstream server").click();
		view.getByTitle("Add secret header").click();
		const accepted = fireEvent.paste(view.getByLabelText("Header name 1"), {
			clipboardData: { getData: () => "x\r\nkey" },
		});
		expect(accepted).toBe(false);
		expect(view.getByText("Invalid or reserved custom header name")).toBeTruthy();
		view.unmount();
	});

	it("rejects custom Authorization when a Bearer value is supplied", async () => {
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Add upstream server").click();
		fireEvent.input(view.getByPlaceholderText("API key for remote MCP servers (optional, stored in OS keychain)"), {
			target: { value: "DUMMY_BEARER" },
		});
		view.getByTitle("Add secret header").click();
		fireEvent.input(view.getByLabelText("Header name 1"), { target: { value: "aUtHoRiZaTiOn" } });
		fireEvent.input(view.getByLabelText("Header secret 1"), { target: { value: "DUMMY_CUSTOM" } });
		view.getByText("Add").click();
		expect(view.getByText("Duplicate custom header or Authorization conflicts with Bearer")).toBeTruthy();
		expect(vi.mocked(rpc).mock.calls.some(([c]) => c === "save_mcp_upstream_credential")).toBe(false);
		view.unmount();
	});

	// The Add/Cancel pair used to share the timeout row, which squeezed them and
	// clipped "Cancel"; they must sit on their own row like the edit form's Save/Cancel.
	it("renders Add and Cancel on a dedicated right-aligned row", async () => {
		const view = render(() => <UpstreamMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);

		view.getByTitle("Add upstream server").click();
		await vi.advanceTimersByTimeAsync(0);

		const addBtn = view.getByText("Add");
		const cancelBtn = view.getByText("Cancel");
		expect(addBtn.parentElement).toBe(cancelBtn.parentElement);
		const row = addBtn.parentElement as HTMLElement;
		expect(row.style.justifyContent).toBe("flex-end");
		// The timeout input must NOT share the row any more.
		expect(row.querySelector('input[type="number"]')).toBeNull();

		view.unmount();
	});
});
