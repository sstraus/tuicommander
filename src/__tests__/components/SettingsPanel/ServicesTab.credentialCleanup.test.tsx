import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: vi.fn().mockResolvedValue(true) }));

import { UpstreamMcpPanel } from "../../../components/SettingsPanel/tabs/services/UpstreamMcpPanel";
import { rpc, type UpstreamMcpServer } from "../../../transport";

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.clearAllMocks();
});

function setup(auth: UpstreamMcpServer["auth"] = { type: "bearer", token: "" }) {
	vi.useFakeTimers();
	const header = { name: "x-api-key", credential_ref: "05622c90-2335-45d7-bf6f-a7132a2a461e" };
	const server: UpstreamMcpServer = {
		id: "saved",
		name: "example",
		transport: { type: "http", url: "https://example.com/mcp" },
		enabled: true,
		timeout_secs: 30,
		headers: [header],
		auth,
	};
	const vault = new Map([
		["example", "DUMMY_AUTH"],
		[header.credential_ref, "DUMMY_HEADER"],
	]);
	let finishSave!: () => void;
	let failSave!: (error: Error) => void;
	const pendingSave = new Promise<void>((resolve, reject) => {
		finishSave = resolve;
		failSave = reject;
	});
	let persisted = [server];
	vi.mocked(rpc).mockImplementation((command, args) => {
		if (command === "load_mcp_upstreams") return Promise.resolve({ servers: persisted });
		if (command === "get_mcp_upstream_status") return Promise.resolve({ upstreams: [] });
		if (command === "save_mcp_upstreams")
			return pendingSave.then(() => {
				expect(args).toBeDefined();
				persisted = (args!.config as { servers: UpstreamMcpServer[] }).servers;
			});
		const key = args?.header ? (args.header as { credential_ref: string }).credential_ref : "example";
		if (command === "delete_mcp_upstream_credential") vault.delete(key);
		if (command === "save_mcp_upstream_credential") vault.set(key, args?.token as string);
		return Promise.resolve(undefined);
	});
	return { header, vault, finishSave, failSave, persisted: () => persisted };
}

it("keeps both credentials until removal is persisted, then deletes them", async () => {
	const state = setup();
	const view = render(() => <UpstreamMcpPanel />);
	try {
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Remove").click();
		await vi.dynamicImportSettled();
		await vi.advanceTimersByTimeAsync(0);
		expect(state.persisted()).toHaveLength(1);
		expect(state.vault.size).toBe(2);
		state.finishSave();
		await vi.advanceTimersByTimeAsync(0);
		expect(state.persisted()).toHaveLength(0);
		expect(state.vault.size).toBe(0);
		expect(view.queryByTitle("Edit")).toBeNull();
	} finally {
		state.finishSave();
		await vi.advanceTimersByTimeAsync(0);
		view.unmount();
	}
});

it.each([
	{ method: "bearer", replace: false },
	{ method: "oauth2", replace: false },
	{ method: "bearer", replace: true },
	{ method: "oauth2", replace: true },
] as const)(
	"preserves the $method credential and removed header after failed config save (replacement=$replace)",
	async ({ method, replace }) => {
		const state = setup(method === "bearer" ? { type: "bearer", token: "" } : { type: "oauth2", client_id: "client" });
		const view = render(() => <UpstreamMcpPanel />);
		try {
			await vi.advanceTimersByTimeAsync(0);
			view.getByTitle("Edit").click();
			if (method === "oauth2" || !replace)
				fireEvent.change(view.getByDisplayValue(method === "bearer" ? "Bearer token" : "OAuth 2.1"), {
					target: { value: method === "bearer" ? "oauth2" : "bearer" },
				});
			if (replace)
				fireEvent.input(view.getByPlaceholderText("Enter new token (leave blank to keep current)"), {
					target: { value: "DUMMY_REPLACEMENT" },
				});
			view.getByTitle("Remove header 1").click();
			view.getByText("Save").click();
			await vi.advanceTimersByTimeAsync(0);
			expect(state.vault.get("example")).toBe("DUMMY_AUTH");
			state.failSave(new Error("Configuration write failed"));
			await vi.advanceTimersByTimeAsync(0);
			expect(state.persisted()[0].auth?.type).toBe(method);
			expect(state.persisted()[0].headers).toEqual([state.header]);
			expect(state.vault.get("example")).toBe("DUMMY_AUTH");
			expect(state.vault.get(state.header.credential_ref)).toBe("DUMMY_HEADER");
			expect(view.getByText("Configuration write failed", { exact: false })).toBeTruthy();
		} finally {
			state.finishSave();
			await vi.advanceTimersByTimeAsync(0);
			view.unmount();
		}
	},
);

it.each([false, true])(
	"cleans up an OAuth credential after saving Bearer without deleting its replacement (%s)",
	async (replace) => {
		const state = setup({ type: "oauth2", client_id: "client" });
		const view = render(() => <UpstreamMcpPanel />);
		try {
			await vi.advanceTimersByTimeAsync(0);
			view.getByTitle("Edit").click();
			fireEvent.change(view.getByDisplayValue("OAuth 2.1"), { target: { value: "bearer" } });
			if (replace)
				fireEvent.input(view.getByPlaceholderText("Enter new token (leave blank to keep current)"), {
					target: { value: "DUMMY_REPLACEMENT" },
				});
			view.getByText("Save").click();
			await vi.advanceTimersByTimeAsync(0);
			state.finishSave();
			await vi.advanceTimersByTimeAsync(0);
			expect(state.persisted()[0].auth?.type).toBe(replace ? "bearer" : undefined);
			expect(state.vault.get("example")).toBe(replace ? "DUMMY_REPLACEMENT" : undefined);
			expect(state.vault.get(state.header.credential_ref)).toBe("DUMMY_HEADER");
			expect(view.queryByText("Save")).toBeNull();
		} finally {
			state.finishSave();
			await vi.advanceTimersByTimeAsync(0);
			view.unmount();
		}
	},
);
