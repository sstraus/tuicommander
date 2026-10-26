import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";

// @ts-expect-error plain MV3 module
import { hasExplicitHostGrant } from "../../../browser-extension/design-mode/browserCommands.js";

const DIR = "browser-extension/design-mode";
const pick = (id: string, selector: string) => ({
	id,
	comment: "",
	raw: { tagName: "button", selector, textContent: id },
});

function mountPanel(snapshot: Record<string, unknown>) {
	vi.useFakeTimers();
	const html = readFileSync(`${DIR}/panel.html`, "utf8");
	document.body.innerHTML = html.slice(html.indexOf("<body>") + 6, html.indexOf("<script"));
	const chrome = {
		permissions: { request: vi.fn(async () => true), getAll: vi.fn(async () => ({ origins: ["<all_urls>"] })) },
		tabs: { query: vi.fn(async () => [{ url: "https://example.com/" }]) },
		runtime: {
			sendMessage: vi.fn(async () => ({ ok: true, result: snapshot })),
			onMessage: { addListener: () => {} },
		},
	};
	new Function("chrome", "hasExplicitHostGrant", readFileSync(`${DIR}/panel.js`, "utf8").replace(/^import .*;\n/, ""))(
		chrome,
		hasExplicitHostGrant,
	);
	return chrome;
}
const base = {
	tabId: 1,
	repoPath: "/repo",
	connection: "Connected · sock",
	agents: [],
	servers: { servers: [], canStart: false },
};

afterEach(() => vi.useRealTimers());

// Catches: the dev-server list pushing pick cards below the fold, and a refresh re-collapsing a list the user opened.
it("dev_server_list_is_collapsed_by_default", async () => {
	const servers = Array.from({ length: 9 }, (_, i) => ({ url: `http://localhost:${5000 + i}/`, source: "listener" }));
	const chrome = mountPanel({ ...base, servers: { servers, canStart: false }, picks: [pick("p1", "#send")] });
	await vi.waitFor(() => expect(document.querySelectorAll("#cards .card")).toHaveLength(1));

	const list = document.getElementById("servers") as HTMLDetailsElement;
	expect(list.tagName).toBe("DETAILS");
	expect(list.open).toBe(false);
	expect(list.querySelector("summary")?.textContent).toBe("Dev servers (9)");
	expect(list.querySelectorAll("button")).toHaveLength(10); // 9 servers + refresh

	list.open = true;
	const calls = chrome.runtime.sendMessage.mock.calls.length;
	await vi.advanceTimersByTimeAsync(5000);
	expect(chrome.runtime.sendMessage.mock.calls.length).toBeGreaterThan(calls);
	expect(list.open).toBe(true);
	expect(list.querySelectorAll("button")).toHaveLength(10);
});

// Catches: render skipping every card while a comment box keeps focus, so later picks are stored but never shown.
it("new_pick_appears_while_comment_has_focus", async () => {
	const snapshot = { ...base, picks: [pick("p1", "#first")] };
	mountPanel(snapshot);
	await vi.waitFor(() => expect(document.querySelectorAll("#cards .card")).toHaveLength(1));
	const box = document.querySelector("#cards textarea") as HTMLTextAreaElement;
	box.focus();
	box.value = "make it smaller";
	box.setSelectionRange(4, 4);

	snapshot.picks = [{ ...pick("p1", "#first"), comment: "make it smaller" }, pick("p2", "#second")];
	await vi.advanceTimersByTimeAsync(5000);

	const cards = document.querySelectorAll("#cards .card");
	expect(Array.from(cards, (card) => card.querySelector("code")?.textContent)).toEqual(["#first", "#second"]);
	expect(document.querySelector("#cards textarea")).toBe(box);
	expect(document.activeElement).toBe(box);
	expect(box.selectionStart).toBe(4);

	snapshot.picks = [pick("p2", "#second")];
	await vi.advanceTimersByTimeAsync(5000);
	expect(Array.from(document.querySelectorAll("#cards .card code"), (code) => code.textContent)).toEqual(["#second"]);
});

// Catches: the Design Mode starter hidden among randomly named agents in the recipient list.
it("starter_agent_is_first_and_labelled", async () => {
	mountPanel({
		...base,
		picks: [],
		agents: [
			{ sessionId: "a", label: "fix-dm-list" },
			{ sessionId: "b", label: "feat-1719" },
		],
		armedSessionId: "b",
		selectedSessionId: "b",
	});
	await vi.waitFor(() => expect(document.querySelectorAll("#agent option")).toHaveLength(2));
	const select = document.getElementById("agent") as HTMLSelectElement;
	expect(select.options[0].textContent).toBe("feat-1719 · started Design Mode");
	expect(select.value).toBe("b");
	expect(select.querySelector("optgroup")?.label).toBe("Other agents");
	expect(Array.from(select.querySelectorAll("optgroup option"), (option) => option.textContent)).toEqual([
		"fix-dm-list",
	]);
});

// Catches: the capture option requesting ordinary host grants instead of optional capture consent.
it("capture_option_requests_optional_all_urls", async () => {
	const chrome = mountPanel({ ...base, picks: [] });
	const button = document.getElementById("grant-capture") as HTMLButtonElement;
	expect(button.textContent).toBe("Let agents capture any site");
	button.click();
	await vi.waitFor(() => expect(chrome.permissions.request).toHaveBeenCalledWith({ origins: ["<all_urls>"] }));
	expect(document.getElementById("grants")?.textContent).toContain("All-sites capture enabled");
});

// Catches: declined capture consent being presented as enabled instead of retaining activeTab recovery.
it("capture_option_decline_preserves_temporary_capture_guidance", async () => {
	const chrome = mountPanel({ ...base, picks: [] });
	chrome.permissions.request.mockResolvedValue(false);
	(document.getElementById("grant-capture") as HTMLButtonElement).click();
	await vi.waitFor(() =>
		expect(document.getElementById("grants")?.textContent).toContain("Capture permission declined"),
	);
	expect(document.getElementById("grants")?.textContent).toContain("browser action");
});

// Catches: Chrome request() returning true without adding eval consent being shown as a grant.
it("host_buttons_do_not_claim_eval_consent_from_capture_only_permission", async () => {
	const chrome = mountPanel({ ...base, picks: [] });
	for (const id of ["grant-host", "grant-all"]) {
		(document.getElementById(id) as HTMLButtonElement).click();
		await vi.waitFor(() =>
			expect(document.getElementById("grants")?.textContent).toContain("Chrome did not add separate eval consent"),
		);
		document.getElementById("grants")!.textContent = "";
	}
	chrome.permissions.getAll.mockResolvedValue({ origins: ["*://example.com/*"] });
	(document.getElementById("grant-host") as HTMLButtonElement).click();
	await vi.waitFor(() =>
		expect(document.getElementById("grants")?.textContent).toBe("Host granted for agent commands and eval."),
	);
});
