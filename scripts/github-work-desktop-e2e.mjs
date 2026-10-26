#!/usr/bin/env node
/** Real desktop/registered-provider E2E. No module replacement or HTTP doubles. */
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { homedir } from "node:os";
import { resolve, sep } from "node:path";

const usage =
	"Usage: node scripts/github-work-desktop-e2e.mjs <isolated-socket> <account-id> <issue-number> <pr-number> --isolated-desktop [--preflight-only]";
const [socket, account, issue, pr, consent, mode, ...extra] = process.argv.slice(2);
if (process.argv.includes("--help")) {
	console.log(usage);
	process.exit(0);
}
if (
	!socket ||
	!resolve(socket).startsWith(resolve(homedir(), "Gits/.tmp/tuic-at") + sep) ||
	!account ||
	!/^[1-9][0-9]*$/.test(issue) ||
	!/^[1-9][0-9]*$/.test(pr) ||
	consent !== "--isolated-desktop" ||
	(mode && mode !== "--preflight-only") ||
	extra.length
) {
	console.error(usage);
	process.exit(2);
}
function request(path, body) {
	return JSON.parse(
		execFileSync(
			"curl",
			[
				"--silent",
				"--show-error",
				"--fail",
				"--max-time",
				"10",
				"--unix-socket",
				socket,
				...(body ? ["-H", "Content-Type: application/json", "--data-binary", JSON.stringify(body)] : []),
				`http://localhost${path}`,
			],
			{ encoding: "utf8", maxBuffer: 1024 * 1024 },
		),
	);
}

async function desktop(input) {
	const checks = [];
	let stage = "loaded plugin";
	const require = (value) => {
		if (!value) throw new Error("check failed");
	};
	const button = (root, text) =>
		[...root.querySelectorAll("button")].find((value) => value.textContent.trim() === text);
	const waitFor = async (read) => {
		const deadline = Date.now() + 35000;
		while (Date.now() < deadline) {
			const value = read();
			if (value) return value;
			await new Promise((done) => setTimeout(done, 100));
		}
		throw new Error("stage timeout");
	};
	try {
		const plugin = window.__TUIC__?.plugin("github-work");
		require(plugin?.loaded && plugin.enabled && !plugin.error);
		checks.push("real user plugin loaded");
		const invoke = window.__TAURI__.core.invoke;
		const config = JSON.parse(await invoke("read_plugin_data", { pluginId: "github-work", path: "work-items.json" }));
		require(
			config.accounts.length === 1 &&
				config.accounts[0].id === input.account &&
				config.accounts[0].repository === "sstraus/tuic-test-sandbox",
		);
		checks.push("sandbox-only registered account configuration");
		stage = "registered provider Tasks UI";
		let shell = document.querySelector('[aria-label="Tasks shell"]');
		if (!shell) {
			await window.__TAURI__.event.emit("menu-action", "command-palette");
			const label = await waitFor(() =>
				[...document.querySelectorAll('[role="dialog"] span')].find((value) => value.textContent === "Open Tasks"),
			);
			label.parentElement.click();
			shell = await waitFor(() => document.querySelector('[aria-label="Tasks shell"]'));
		}
		require(button(shell, "New task") && shell.querySelector('[aria-label="Task results"]'));
		checks.push("real registered provider mounted in Tasks");
		if (input.preflight)
			return {
				marker: input.marker,
				ok: true,
				preflightOnly: true,
				checks,
				limits: "No authenticated provider, B6/B7 or GitHub write verified.",
			};
		stage = "account-scoped list";
		button(shell, "Refresh").click();
		const row = (number) =>
			[...shell.querySelectorAll('[aria-label="Task results"] [role="listitem"] button')].find((value) =>
				value.textContent.includes(`sstraus/tuic-test-sandbox#${number}`),
			);
		await waitFor(() => row(input.issue) && row(input.pr));
		checks.push("authenticated sandbox issue and PR list");
		for (const [number, kind] of [
			[input.issue, "issue"],
			[input.pr, "pull"],
		]) {
			stage = `${kind} registered-provider detail`;
			row(number).click();
			const detail = await waitFor(() => {
				const value = shell.querySelector('[aria-label="Task detail"]');
				return value?.querySelector("h2") && value.textContent.includes(`sstraus/tuic-test-sandbox#${number}`) && value;
			});
			require(detail.querySelector('[aria-label="Description"]') && detail.querySelector('[aria-label="Comments"]'));
			checks.push(`${kind} detail and comments from registered provider`);
			if (kind === "pull") {
				stage = "B6 real provider review-thread pagination";
				for (let page = 0; page < 4 && !detail.querySelector('[aria-label="Review thread"]'); page++) {
					const more = button(detail, "Load more comments");
					if (!more) break;
					more.click();
					await waitFor(() => !button(detail, "Loading comments…"));
				}
				await waitFor(() => detail.querySelector('[aria-label="Review thread"]'));
				checks.push("B6 review thread through registered provider and real host");
			}
			button(shell, "Back to list").click();
		}
		stage = "B7 account-scoped state port";
		const state = await invoke("plugin_github_action", {
			pluginId: "github-work",
			account: input.account,
			reference: { providerId: "github", accountId: input.account, id: `sstraus/tuic-test-sandbox/pull/${input.pr}` },
			request: { action: "state" },
		});
		require(state && typeof state === "object");
		checks.push("B7 account-scoped core state read (no PR mutation)");
		return {
			marker: input.marker,
			ok: true,
			preflightOnly: false,
			checks,
			limits: "B7 uses the production IPC port directly; no Projects, start-work or mutation coverage claimed.",
		};
	} catch {
		return { marker: input.marker, ok: false, stage, checks };
	}
}

const marker = `github-work-desktop-${randomUUID()}`;
try {
	const health = request("/health");
	if (!health.ok || health.socket_path !== socket) throw new Error("Selected isolated socket identity mismatch");
	const input = { marker, account, issue, pr, preflight: mode === "--preflight-only" };
	const submission = request("/debug/invoke_js", {
		script: `return (${desktop.toString()})(${JSON.stringify(input)});`,
	});
	if (!submission.ok) throw new Error("Desktop evaluator unavailable");
	const deadline = Date.now() + 450000;
	while (Date.now() < deadline) {
		for (const log of request("/logs?source=eval_js&limit=30")) {
			let result;
			try {
				result = JSON.parse(log.message.split("\n---\n").at(-1));
			} catch {
				continue;
			}
			if (result?.marker !== marker) continue;
			console.log(JSON.stringify(result, null, 2));
			process.exit(result.ok ? 0 : 1);
		}
		await new Promise((done) => setTimeout(done, 250));
	}
	throw new Error("Desktop E2E result deadline exceeded");
} catch {
	console.error("Isolated desktop E2E transport failed; server response withheld.");
	process.exit(1);
}
