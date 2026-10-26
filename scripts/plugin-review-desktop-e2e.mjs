#!/usr/bin/env node
/** Real installed-plugin checks; no host, catalogue, credential or network doubles. */
import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve, sep } from "node:path";

const [socket, instance, consent, ...extra] = process.argv.slice(2);
if (
	!socket ||
	!resolve(socket).startsWith(resolve(homedir(), "Gits/.tmp/tuic-at") + sep) ||
	!/^agent-test-[a-z0-9-]+$/.test(instance ?? "") ||
	consent !== "--isolated-desktop" ||
	extra.length
) {
	throw new Error(
		"Usage: node scripts/plugin-review-desktop-e2e.mjs <isolated-socket> <agent-test-name> --isolated-desktop",
	);
}
const instanceHash = createHash("sha256").update(instance).digest("hex").slice(0, 16);
if (resolve(socket) !== join(homedir(), "Gits/.tmp/tuic-at", instanceHash, `tuic-mcp-${instanceHash}.sock`))
	throw Error("Instance/socket mismatch");
const root = resolve(import.meta.dirname, "..");
const assets = readdirSync(join(root, "dist/assets"));
const asset = (prefix) =>
	`/assets/${
		assets.find((name) => name.startsWith(prefix) && name.endsWith(".js")) ??
		(
			() => {
				throw Error(`Missing ${prefix} asset`);
			}
		)()
	}`;
const modules = {
	loader: asset("pluginLoader-"),
	registry: asset("markdownProviderRegistry-"),
	app: asset("appEntry-"),
};
// Resolve the existing reload export from this build, not a separate source-module instance.
const loaderSource = readFileSync(join(root, "dist", modules.loader), "utf8");
const reloadSymbol = loaderSource.match(/function (\w+)\((\w+)\)\{return \w+\(\{payload:\[\2\]\}\)\}/)?.[1];
const reloadExport = loaderSource.match(new RegExp(`(?:\\{|,)${reloadSymbol} as (\\w+)`))?.[1];
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
async function evaluate(fn, input = {}) {
	const marker = randomUUID();
	const result = request("/debug/invoke_js", {
		script: `return(async()=>{try{return{marker:${JSON.stringify(marker)},value:await(${fn.toString()})(${JSON.stringify({ ...input, modules, reloadExport, root })})}}catch(e){return{marker:${JSON.stringify(marker)},error:String(e)}}})();`,
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
	throw Error("WebView result deadline exceeded");
}
async function setup(input) {
	const invoke = window.__TAURI__.core.invoke;
	const loader = await import(input.modules.loader);
	const registry = Object.values(await import(input.modules.registry)).find(
		(value) => value?.list && value?.get && value?.register && value.list().some((p) => p.providerId === "github"),
	);
	if (!registry) throw Error("Real GitHub provider registry missing");
	const wait = async (fn) => {
		for (let i = 0; i < 300; i++) {
			const value = fn();
			if (value) return value;
			await new Promise((done) => setTimeout(done, 100));
		}
		throw Error("Settings control timeout");
	};
	const outcome = async (operation) => {
		try {
			return { value: await operation() };
		} catch (error) {
			return { error: String(error) };
		}
	};
	const results = {};
	for (const spec of [
		{ id: "jira", field: "projectId", label: "Default project ID", project: "TUIC-1756" },
		{ id: "gitlab", field: "project", label: "Default project path or ID", project: "sstraus/tuic-1756" },
	]) {
		await invoke("install_plugin_from_folder", { path: `${input.root}/plugins/${spec.id}` });
		await invoke("write_plugin_data", {
			pluginId: spec.id,
			path: "work-items.json",
			content: JSON.stringify({
				accounts: [{ id: "1756-local-no-credential", host: "https://tuic-e2e.invalid", kind: "bearer" }],
			}),
		});
		await loader[input.reloadExport](spec.id);
		const open = async () => {
			if (![...document.querySelectorAll("button")].some((b) => b.textContent.trim() === "Manage plugins"))
				await window.__TAURI__.event.emit("menu-action", "settings");
			(await wait(() => document.querySelector('button[title="Plugin: ' + spec.id + '"]'))).click();
			return wait(() =>
				[...document.querySelectorAll("input")].find((value) =>
					[...document.querySelectorAll("label")].some(
						(label) => label.htmlFor === value.id && label.textContent === spec.label,
					),
				),
			);
		};
		const write = async (value) => {
			const field = await open();
			if (field.value === value) return;
			field.value = value;
			field.dispatchEvent(new Event("input", { bubbles: true }));
			field.dispatchEvent(new FocusEvent("blur"));
			for (let i = 0; i < 300; i++) {
				const raw = await invoke("read_plugin_data", { pluginId: spec.id, path: "config.json" });
				if (raw && JSON.parse(raw)[spec.field] === value) return;
				await new Promise((done) => setTimeout(done, 100));
			}
			throw Error(`${spec.id} native settings persistence timeout`);
		};
		const consume = () =>
			registry.get(spec.id, "1756-local-no-credential").createFields(spec.id === "gitlab" ? "issue" : undefined);
		await write("");
		await loader[input.reloadExport](spec.id);
		const before = await outcome(consume);
		await write(spec.project);
		const saved = JSON.parse(await invoke("read_plugin_data", { pluginId: spec.id, path: "config.json" }));
		await loader[input.reloadExport](spec.id);
		const after = await outcome(consume);
		results[spec.id] = { before, saved, after, loaded: window.__TUIC__.plugin(spec.id) };
	}
	return results;
}
async function ports(input) {
	const registry = Object.values(await import(input.modules.registry)).find(
		(value) => value?.list && value?.get && value?.register && value.list().some((p) => p.providerId === "github"),
	);
	const outcome = async (operation) => {
		try {
			return { value: await operation() };
		} catch (error) {
			return { error: String(error) };
		}
	};
	const github = registry.get("github", "sstraus");
	const reference = { providerId: "github", accountId: "sstraus", id: "sstraus/tuic-test-sandbox/pull/2" };
	const threads = await outcome(() => github.comments(reference, "threads:first", 50));
	const state = await outcome(() => github.runPrAction(reference, "state"));
	const mismatch = await outcome(() => github.runPrAction({ ...reference, accountId: "other" }, "state"));
	return { threads, state, mismatch, loaded: window.__TUIC__.plugin("github-work") };
}
async function catalogue(input) {
	const registry = Object.values(await import(input.modules.app)).find(
		(value) => value?.hasUpdate && value?.refresh && value?.state?.entries,
	);
	if (!registry) throw Error("Production catalogue singleton missing");
	await registry.refresh();
	const entry = registry.state.entries.find((entry) => entry.id === "github-work");
	const plugin = window.__TUIC__.plugin("github-work");
	const button = [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Manage plugins");
	if (!button) {
		await window.__TAURI__.event.emit("menu-action", "settings");
		for (let i = 0; i < 300; i++) {
			const opened = [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Manage plugins");
			if (opened) {
				opened.click();
				break;
			}
			await new Promise((done) => setTimeout(done, 100));
		}
	} else button.click();
	await new Promise((done) => setTimeout(done, 100));
	const row = [...document.querySelectorAll('[class*="_pluginRow"]')].find((value) =>
		value.textContent.includes("GitHub Work"),
	);
	return {
		error: registry.state.error,
		entry,
		plugin,
		update: registry.hasUpdate("github-work", plugin.manifest.version, plugin.manifest.contentSha256),
		row: row?.innerText,
		updateButton: !!row && [...row.querySelectorAll("button")].some((b) => b.textContent.trim() === "Update"),
	};
}
const health = request("/health");
if (!health.ok || health.socket_path !== socket) throw Error("Isolated socket identity mismatch");
const report = { health, modules, reloadExport };
report.candidate12 = await evaluate(ports);
try {
	report.candidate9 = await evaluate(setup);
} catch (error) {
	report.candidate9 = { error: String(error) };
}
report.initial = await evaluate(catalogue);
const payload = join(
	homedir(),
	"Library/Application Support/com.tuic.commander/instances",
	instance,
	"plugins/github-work/main.js",
);
if (
	!realpathSync(payload).startsWith(
		join(homedir(), "Library/Application Support/com.tuic.commander/instances", instance) + sep,
	)
)
	throw Error("Payload escapes own instance");
const original = readFileSync(payload);
try {
	writeFileSync(
		payload,
		Buffer.concat([original, Buffer.from("\n// 1756 harmless equal-version payload drift probe.\n")]),
	);
	await evaluate(async (input) => {
		await (await import(input.modules.loader))[input.reloadExport]("github-work");
		return true;
	});
	report.altered = await evaluate(catalogue);
} finally {
	writeFileSync(payload, original);
	await evaluate(async (input) => {
		await (await import(input.modules.loader))[input.reloadExport]("github-work");
		return true;
	});
}
report.restored = await evaluate(catalogue);
report.candidate4 =
	report.altered.entry?.contentSha256 &&
	report.altered.entry.latestVersion === report.altered.plugin.manifest.version &&
	report.altered.plugin.manifest.contentSha256 !== report.altered.entry.contentSha256 &&
	!!report.altered.update &&
	report.altered.updateButton;
console.log(JSON.stringify(report, null, 2));
const settings = report.candidate9;
const settingsBoundary =
	settings?.jira?.before.error === "Error: Jira project is required" &&
	settings.jira.saved.projectId === "TUIC-1756" &&
	settings.jira.after.error === "Account not found" &&
	settings?.gitlab?.before.error === "Error: GitLab create needs a configured project" &&
	settings.gitlab.saved.project === "sstraus/tuic-1756" &&
	settings.gitlab.after.error === "Account not found";
const portsBoundary =
	report.candidate12.threads.error === "Account not found" &&
	report.candidate12.state.error === "Account not found" &&
	report.candidate12.mismatch.error === "Error: Item belongs to another provider/account";
if (
	!report.candidate4 ||
	report.initial.updateButton ||
	report.restored.updateButton ||
	report.initial.plugin.manifest.contentSha256 !== report.restored.plugin.manifest.contentSha256 ||
	!settingsBoundary ||
	!portsBoundary
)
	process.exitCode = 1;
