import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { beforeEach, describe, expect, it, vi } from "vitest";

/** Run the real `public/sw.js` and return the handler for `fetch` plus the cache writes. */
function loadServiceWorker(networkResponse: Response) {
	const handlers = new Map<string, (event: unknown) => void>();
	const writes: string[] = [];
	runInNewContext(readFileSync("public/sw.js", "utf8"), {
		self: { addEventListener: (type: string, handler: (event: unknown) => void) => handlers.set(type, handler) },
		caches: { open: async () => ({ put: async (key: string) => void writes.push(key) }), match: async () => undefined },
		fetch: async () => networkResponse,
		Response,
		Request,
		URL,
		AbortController,
		setTimeout,
		clearTimeout,
	});
	async function navigate(path: string) {
		let answer: Promise<Response> | undefined;
		handlers.get("fetch")?.({
			request: { url: `http://tuic.test${path}`, method: "GET", mode: "navigate" },
			respondWith: (p: Promise<Response>) => {
				answer = p;
			},
		});
		expect(await answer).toBe(networkResponse);
	}
	return { navigate, writes };
}

describe("service worker shell cache", () => {
	// Plausible bug: the login page is a 200 under /mobile/, so it overwrote the
	// cached app shell and an offline launch opened a form that cannot log in.
	it("never stores the login page as the offline shell", async () => {
		const { navigate, writes } = loadServiceWorker(new Response("<form>", { status: 200 }));
		await navigate("/mobile/login?next=%2Fmobile");
		expect(writes).toEqual([]);
	});

	// Catches: successful navigation writes an unused shell instead of keeping offline recovery splash-only.
	it("does not cache the app shell on a successful /mobile navigation", async () => {
		const { navigate, writes } = loadServiceWorker(new Response("<app>", { status: 200 }));
		await navigate("/mobile");
		expect(writes).toEqual([]);
	});

	// Plausible bug: a cached refusal replaces the shell and the app never leaves it.
	it.each([401, 403, 429])("never stores a %i as the offline shell", async (status) => {
		const { navigate, writes } = loadServiceWorker(new Response("no", { status }));
		await navigate("/mobile");
		await navigate("/mobile/session/a");
		expect(writes).toEqual([]);
	});
});

describe("mobile login form", () => {
	const html = readFileSync("public/mobile-login.html", "utf8");
	const script = readFileSync("public/mobile-login.js", "utf8");
	const replace = vi.fn();

	function mount(search: string, response: Response | Error, hash = "") {
		document.documentElement.innerHTML = html.replace(/<script[\s\S]*?<\/script>/, "");
		const fetchMock = vi.fn(async () => {
			if (response instanceof Error) throw response;
			return response;
		});
		runInNewContext(script, {
			document,
			fetch: fetchMock,
			location: { search, hash, replace },
			URLSearchParams,
			JSON,
		});
		return fetchMock;
	}
	async function submit(user: string, pass: string) {
		(document.getElementById("username") as HTMLInputElement).value = user;
		(document.getElementById("password") as HTMLInputElement).value = pass;
		document.getElementById("login")?.dispatchEvent(new Event("submit", { cancelable: true }));
		await new Promise((r) => setTimeout(r, 0));
	}

	beforeEach(() => replace.mockReset());

	it("posts the credentials as JSON with the return path and follows the server's next", async () => {
		const fetchMock = mount("?next=%2Fmobile%2Fsession%2Fa", Response.json({ ok: true, next: "/mobile/session/a" }));
		await submit("boss", "pw");
		const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
		expect(url).toBe("/auth/login");
		expect(init.method).toBe("POST");
		expect((init.headers as Record<string, string>)["Content-Type"]).toBe("application/json");
		expect(JSON.parse(init.body as string)).toEqual({ username: "boss", password: "pw", next: "/mobile/session/a" });
		expect(replace).toHaveBeenCalledWith("/mobile/session/a");
	});

	// Catches: root form login drops the fragment before iPad tablet routing can preserve it.
	it("preserves the root query and inherited fragment after login", async () => {
		mount("?next=%2F%3Fshared%3Dk", Response.json({ ok: true, next: "/?shared=k" }), "#sessions");
		await submit("boss", "pw");
		expect(replace).toHaveBeenCalledWith("/?shared=k#sessions");
	});

	// Plausible bug: a rejected login redirects anyway, or leaves the button dead.
	it.each([
		[401, "Wrong username or password."],
		[429, "Too many attempts. Try again later."],
		[403, "Login refused: open this page from the TUICommander address."],
	])("shows the %i refusal and stays on the page", async (status, text) => {
		mount("", new Response(JSON.stringify({ error: "x" }), { status }));
		await submit("boss", "bad");
		expect(document.getElementById("error")?.textContent).toBe(text);
		expect((document.getElementById("submit") as HTMLButtonElement).disabled).toBe(false);
		expect(replace).not.toHaveBeenCalled();
	});

	it("reports an unreachable server and re-enables the button", async () => {
		mount("", new Error("offline"));
		await submit("boss", "pw");
		expect(document.getElementById("error")?.textContent).toBe("Server unreachable. Try again.");
		expect((document.getElementById("submit") as HTMLButtonElement).disabled).toBe(false);
	});
});
