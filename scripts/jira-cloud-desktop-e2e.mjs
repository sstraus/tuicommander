#!/usr/bin/env node
/** Real installed Jira provider, native settings UI and authorized scratch-site writes. */
import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

const [instance, accountId, projectId, issueType, output, consent, ...extra] = process.argv.slice(2);
if (
	!/^agent-test-[a-z0-9-]+$/.test(instance ?? "") ||
	!accountId ||
	!/^\d+$/.test(projectId ?? "") ||
	!/^\d+$/.test(issueType ?? "") ||
	!output ||
	!["--write-straus-team-only", "--read-only-resolver"].includes(consent) ||
	(consent === "--read-only-resolver" && extra.length > 0) ||
	extra.length > 1 ||
	(extra.length === 1 && !/^--resume=TUIC-\d+$/.test(extra[0]))
) {
	throw Error(
		"Usage: jira-cloud-desktop-e2e.mjs <agent-test-name> <account-id> <project-id> <issue-type-id> <output.json> --write-straus-team-only [--resume=TUIC-N] (or --read-only-resolver)",
	);
}
const root = resolve(import.meta.dirname, "..");
if (!resolve(output).startsWith(join(root, "docs") + sep)) throw Error("Evidence must stay under docs/");
const hash = createHash("sha256").update(instance).digest("hex").slice(0, 16);
// make agent-test's socket layout (scripts/agent-test.py): <root>/tuic-at/<hash:8>/tuic-mcp-<hash:16>.sock.
const agentTestRoot = process.env.TUIC_AGENT_TEST_ROOT || join(process.env.TUIC_TEST_HOST_TMPDIR || tmpdir(), "tuic-at");
const socket = join(agentTestRoot, hash.slice(0, 8), `tuic-mcp-${hash}.sock`);
const files = readdirSync(join(root, "dist/assets"));
const asset = (prefix) => {
	const name = files.find((name) => name.startsWith(prefix) && name.endsWith(".js"));
	if (!name) throw Error(`Missing ${prefix} asset`);
	return `/assets/${name}`;
};
const modules = { loader: asset("pluginLoader-"), registry: asset("markdownProviderRegistry-") };
const source = readFileSync(join(root, "dist", modules.loader), "utf8");
const symbol = source.match(/function (\w+)\((\w+)\)\{return \w+\(\{payload:\[\2\]\}\)\}/)?.[1];
const reloadExport = source.match(new RegExp(`(?:\\{|,)${symbol} as (\\w+)`))?.[1];
if (!reloadExport) throw Error("Cannot resolve production reload export");
function request(path, body) {
	return JSON.parse(
		execFileSync(
			"curl",
			[
				"--silent",
				"--show-error",
				"--fail",
				"--max-time",
				"15",
				"--unix-socket",
				socket,
				...(body ? ["-H", "Content-Type: application/json", "--data-binary", JSON.stringify(body)] : []),
				`http://localhost${path}`,
			],
			{ encoding: "utf8", maxBuffer: 2 ** 22 },
		),
	);
}
async function evaluate(fn, input) {
	const marker = randomUUID();
	const result = request("/debug/invoke_js", {
		script: `return(async()=>{try{return{marker:${JSON.stringify(marker)},value:await(${fn.toString()})(${JSON.stringify(input)})}}catch(e){return{marker:${JSON.stringify(marker)},error:String(e)}}})();`,
	});
	if (!result.ok) throw Error(JSON.stringify(result));
	const deadline = Date.now() + 120000;
	while (Date.now() < deadline) {
		for (const entry of request("/logs?source=eval_js&limit=100")) {
			let result;
			try {
				result = JSON.parse(entry.message.split("\n---\n").at(-1));
			} catch {
				continue;
			}
			if (result?.marker !== marker) continue;
			if (result.error) throw Error(result.error);
			return result.value;
		}
		await new Promise((done) => setTimeout(done, 200));
	}
	throw Error("Native WebView result deadline exceeded");
}
const health = request("/health");
if (!health.ok || health.socket_path !== socket) throw Error("Wrong isolated instance");
const result = await evaluate(
	async (input) => {
		const invoke = window.__TAURI__.core.invoke;
		const host = "https://straus-team.atlassian.net";
		const loader = await import(input.modules.loader);
		await invoke("install_plugin_from_folder", { path: `${input.root}/plugins/jira` });
		// A different baseline project makes the actual Settings input the only way to reach the target.
		await invoke("write_plugin_data", {
			pluginId: "jira",
			path: "config.json",
			content: JSON.stringify({ projectId: input.baseline }),
		});
		await invoke("write_plugin_data", {
			pluginId: "jira",
			path: "work-items.json",
			content: JSON.stringify({ accounts: [{ id: input.accountId, host, kind: "basic" }] }),
		});
		await loader[input.reloadExport]("jira");
		const registry = Object.values(await import(input.modules.registry)).find(
			(value) =>
				value?.list && value?.get && value?.register && value.list().some((provider) => provider.providerId === "jira"),
		);
		if (!registry) throw Error("Production Jira provider registry not found");
		const wait = async (fn) => {
			for (let i = 0; i < 300; i++) {
				const value = fn();
				if (value) return value;
				await new Promise((done) => setTimeout(done, 100));
			}
			throw Error("Native settings control deadline exceeded");
		};
		if (![...document.querySelectorAll("button")].some((button) => button.textContent.trim() === "Manage plugins"))
			await window.__TAURI__.event.emit("menu-action", "settings");
		(await wait(() => document.querySelector('button[title="Plugin: jira"]'))).click();
		const field = await wait(() =>
			[...document.querySelectorAll("input")].find((field) =>
				[...document.querySelectorAll("label")].some(
					(label) => label.htmlFor === field.id && label.textContent === "Default project ID",
				),
			),
		);
		const before = await invoke("read_plugin_data", { pluginId: "jira", path: "config.json" });
		if (JSON.parse(before || "{}").projectId !== input.baseline) throw Error("Settings baseline was not seeded");
		field.value = input.projectId;
		field.dispatchEvent(new Event("input", { bubbles: true }));
		field.dispatchEvent(new FocusEvent("blur"));
		let saved;
		for (let i = 0; i < 300; i++) {
			const raw = await invoke("read_plugin_data", { pluginId: "jira", path: "config.json" });
			if (raw && JSON.parse(raw).projectId === input.projectId) {
				saved = JSON.parse(raw);
				break;
			}
			await new Promise((done) => setTimeout(done, 100));
		}
		if (!saved) throw Error("Native settings did not persist project ID");
		await loader[input.reloadExport]("jira");
		// The native file watcher debounces 500 ms; wait for the final registered
		// instance instead of racing its automatic reload with vendor requests.
		let lastProvider;
		let stableSince = performance.now();
		await wait(() => {
			const current = registry.get("jira", input.accountId);
			if (!current || current !== lastProvider) {
				lastProvider = current;
				stableSince = performance.now();
			}
			return current && performance.now() - stableSince >= 2000;
		});
		const provider = registry.get("jira", input.accountId);
		if (!provider || registry.list("jira").length !== 1) throw Error("Expected exactly one production Jira provider");
		if (!provider.resolveUrl(host + "/browse/TUIC-1")) throw Error("Provider is not bound to authorized site");
		const types = await provider.createFields();
		if (!types[0].options.some((type) => type.id === input.issueType))
			throw Error("Vendor rejected configured project/type");
		const schema = input.resolverOnly ? [] : await provider.createFields(input.issueType);
		if (input.resolverOnly) {
			const exports = Object.values(await import(input.modules.registry));
			const urls = exports.find(
				(value) => value?.register && value?.resolve?.toString().includes("URL resolver failed"),
			);
			if (!urls) throw Error("Production URL resolver registry not found");
			const url = host + "/browse/TUIC-3";
			const resolved = await urls.resolve(url, new AbortController().signal);
			if (
				resolved?.link.key !== "TUIC-3" ||
				resolved.link.reference.accountId !== input.accountId ||
				resolved.startWork?.delivery !== "draft"
			)
				throw Error("Registered Jira URL did not resolve");
			const controller = new AbortController();
			let settled = false;
			const started = performance.now();
			const pending = urls.resolve(url, controller.signal).finally(() => {
				settled = true;
			});
			// A real in-flight read is required: immediate transport failure is not cancellation.
			await new Promise((done) => setTimeout(done, 25));
			if (settled) throw Error("Resolver settled before cancellation; no in-flight request proven");
			const abortedAt = performance.now();
			controller.abort();
			if ((await pending) !== null) throw Error("Cancelled resolver returned a link");
			const cancelMs = performance.now() - abortedAt;
			if (cancelMs >= 1000) throw Error(`Cancellation took ${cancelMs} ms (native HTTP deadline is 30000 ms)`);
			if ((await urls.resolve(url, controller.signal)) !== null) throw Error("Pre-aborted URL returned a link");
			const cancellation = {
				pendingAtAbort: true,
				abortAfterMs: abortedAt - started,
				settledAfterAbortMs: cancelMs,
				cancelledLink: null,
				preabortedLink: null,
				evidence:
				"native aborted-result suppression only: the host throws AbortError after native HTTP settles; request stop is proven by the module/transport cancellation tests, not here",
			};
			const recovered = await urls.resolve(url, new AbortController().signal);
			if (recovered?.link.key !== "TUIC-3") throw Error("Normal URL resolution did not recover after cancel");
			return {
				settings: { source: "actual input and blur", savedProjectId: saved.projectId },
				provider: { providerId: provider.providerId, registrationCount: registry.list("jira").length },
				metadataAccepted: types[0].options.some((type) => type.id === input.issueType),
				resolved,
				cancellation,
				recovered: recovered.link.key,
			};
		}
		const body = "Native plugin creation\n\n- First\n- Second\n\n```js\nconst n = 1;\n```\n\nHard  \nbreak";
		const created = input.resume
			? {
					kind: "item",
					value: await provider.detail({ providerId: "jira", accountId: input.accountId, id: input.resume }),
				}
			: await provider.act(null, {
					kind: "create",
					value: {
						title: "Native Jira plugin E2E",
						body,
						itemType: input.issueType,
						fields: { labels: { kind: "labels", value: [{ id: "native-e2e", name: "native-e2e" }] } },
					},
				});
		const ref = created.value.item.reference;
		if (!ref.id.startsWith("TUIC-")) throw Error("Write escaped scratch project");
		if (created.value.body !== body) throw Error("Post-create ADF read-back changed Markdown");
		const comment = await provider.act(ref, { kind: "comment", value: "Native provider comment with hard  \nbreak" });
		if (
			!comment.value.comments.entries.some((entry) => entry.value.body === "Native provider comment with hard  \nbreak")
		)
			throw Error("Post-comment read-back missing");
		const done = comment.value.actions.find((action) => action.kind === "transition" && action.name === "Done");
		if (!done) throw Error("No legal Done transition");
		const moved = await provider.act(ref, { kind: "transition", value: done.id });
		if (moved.value.item.state.name !== "Done") throw Error("Transition did not persist");
		const query = { search: `key = ${ref.id}`, filters: [], savedFilterId: null, sort: null, cursor: null, limit: 25 };
		const page = await provider.list(query);
		if (!page.items.some((item) => item.reference.id === ref.id)) throw Error("Created item not searchable");
		return {
			settings: { source: "actual input and blur", savedProjectId: saved.projectId },
			provider: { providerId: provider.providerId, registrationCount: registry.list("jira").length },
			metadata: {
				issueTypes: types[0].options,
				fields: schema.map((field) => ({ id: field.id, required: field.required })),
			},
			created: {
				resumedAfterCreate: !!input.resume,
				key: created.value.item.key,
				url: created.value.item.url,
				title: created.value.item.title,
				body: created.value.body,
			},
			commentReadBack: true,
			transition: { id: done.id, state: moved.value.item.state.name },
			searchable: true,
			accountBound: true,
			pluginSource: input.root + "/plugins/jira",
		};
	},
	{
		root,
		modules,
		reloadExport,
		accountId,
		projectId,
		baseline: projectId === "99999" ? "99998" : "99999",
		issueType,
		resume: extra[0]?.slice("--resume=".length),
		resolverOnly: consent === "--read-only-resolver",
	},
);
const report = { date: new Date().toISOString(), instance, socket, native: "aarch64-apple-darwin", result };
writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify(report, null, 2));
