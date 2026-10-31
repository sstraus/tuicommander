import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appLogger } from "../../../stores/appLogger";
import { cleanupToasts } from "../../helpers/toasts";
import "../../mocks/tauri";

import { AiChatTab } from "../../../components/SettingsPanel/tabs/AiChatTab";
import type { EgoCliClient } from "../../../services/egoCli";
import { HttpRpcError } from "../../../transport";
import type { EgoCliError, EgoProvider, EgoProviders } from "../../../types/ego";

/** The projection `ego_cli.rs` sends, with only the parts a case cares about. */
function snapshot(providers: EgoProvider[], defaultModel: string | null = null): EgoProviders {
	return { defaultModel, providers };
}

function provider(name: string, models: EgoProvider["models"], credential?: EgoProvider["credential"]): EgoProvider {
	return { name, credential: credential ?? { state: "stored", detail: "" }, models };
}

function model(slug: string, available = true, unavailable: string | null = null) {
	return { slug, name: slug.split("/").slice(1).join("/"), available, unavailable };
}

function failure(over: Partial<EgoCliError>): EgoCliError {
	return { code: "commandFailed", message: "", command: "", stdout: "", stderr: "", exitCode: null, ...over };
}

/** A client that answers from a script, recording what the tab asked for. */
function fakeClient(over: Partial<EgoCliClient> = {}): EgoCliClient & { refreshes: boolean[]; written: string[] } {
	const refreshes: boolean[] = [];
	const written: string[] = [];
	return {
		refreshes,
		written,
		providers: async (refresh: boolean) => {
			refreshes.push(refresh);
			return snapshot([]);
		},
		setDefaultModel: async (slug: string) => {
			written.push(slug);
			return snapshot([]);
		},
		...over,
	};
}

describe("AiChatTab — diagnostic independence", () => {
	it.each([
		[
			"doctor exit 1 with JSON",
			"ego health check reported problems: session store: probe timed out: the two-second doctor budget was exhausted",
		],
		[
			"doctor exit 1 without JSON",
			"ego health check failed (exit 1); credential status is unavailable. Run ego doctor --json in a terminal.",
		],
		["doctor ok", null],
	])("keeps model selection usable with %s", async (_case, diagnostic) => {
		// Catches: a doctor warning replacing the model selector, or being silently dropped.
		const loaded = {
			...snapshot(
				[
					provider(
						"openai-codex",
						[model("openai-codex/gpt-6.1-sol"), model("openai-codex/gpt-6.1-terra")],
						_case.includes("without JSON") ? { state: "unknown", detail: "Health status unavailable" } : undefined,
					),
				],
				"openai-codex/gpt-6.1-sol",
			),
			diagnostic,
		};
		const client = fakeClient({
			providers: async () => loaded,
			setDefaultModel: vi.fn(async (model: string) => ({ ...loaded, defaultModel: model })),
		});
		render(() => <AiChatTab client={client} />);
		const select = await screen.findByRole("combobox", { name: "Default model" });
		expect((select as HTMLSelectElement).value).toBe("openai-codex/gpt-6.1-sol");
		expect((select as HTMLSelectElement).disabled).toBe(false);
		if (diagnostic) expect(await screen.findByText(diagnostic)).toBeTruthy();
		else expect(screen.queryByRole("status")).toBeNull();
		expect(screen.queryByText("ego refused the command")).toBeNull();
		fireEvent.change(select, { target: { value: "openai-codex/gpt-6.1-terra" } });
		await vi.waitFor(() => expect(client.setDefaultModel).toHaveBeenCalledWith("openai-codex/gpt-6.1-terra"));
		await vi.waitFor(() => expect((select as HTMLSelectElement).value).toBe("openai-codex/gpt-6.1-terra"));
	});
});

describe("AiChatTab", () => {
	it("keeps model controls without exposing ownership and transport commentary", async () => {
		const client = fakeClient({
			providers: async () =>
				snapshot([provider("anthropic", [model("anthropic/claude-opus-5")])], "anthropic/claude-opus-5"),
		});
		const { container } = render(() => <AiChatTab client={client} />);
		expect(await screen.findByRole("combobox")).toBeTruthy();
		expect(screen.getByRole("button", { name: "Refresh from providers" })).toBeTruthy();
		expect(screen.queryByText("New ego runs")).toBeNull();
		// Catches implementation prose returning beside otherwise functional controls.
		expect(container.textContent).not.toMatch(
			/TUICommander owns|stored by ego|re-enumerate its sources|Credential state comes from/,
		);
	});

	it("lists every provider ego reported, with its models", async () => {
		const client = fakeClient({
			providers: async () =>
				snapshot([
					provider("anthropic", [model("anthropic/claude-opus-5")]),
					provider("openrouter", [model("openrouter/z-ai/glm-4.7")]),
				]),
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText("anthropic")).toBeTruthy();
		expect(screen.getByText("openrouter")).toBeTruthy();
		// The name is the half after the first separator, split in Rust — a
		// provider-qualified slug keeps the rest of its path.
		expect(screen.getByRole("option", { name: "claude-opus-5" })).toBeTruthy();
		expect(screen.getByRole("option", { name: "z-ai/glm-4.7" })).toBeTruthy();
	});

	it("opens without asking ego to reach the network", async () => {
		const client = fakeClient();
		render(() => <AiChatTab client={client} />);

		await screen.findByText(/ego knows no providers yet/);
		expect(client.refreshes).toEqual([false]);
	});

	it("asks for a refresh only when the button is pressed", async () => {
		const client = fakeClient();
		render(() => <AiChatTab client={client} />);
		await screen.findByText(/ego knows no providers yet/);

		fireEvent.click(screen.getByRole("button", { name: "Refresh from providers" }));

		await vi.waitFor(() => expect(client.refreshes).toEqual([false, true]));
	});

	it("shows the default model ego holds, not the one that was sent", async () => {
		const models = [model("anthropic/claude-opus-5"), model("anthropic/claude-haiku-4-5")];
		// ego answers the write with a different model than the one requested.
		// The tab must render ego's answer: that is the whole point of re-reading.
		const client = fakeClient({
			providers: async () => snapshot([provider("anthropic", models)], "anthropic/claude-opus-5"),
			setDefaultModel: async () => snapshot([provider("anthropic", models)], "anthropic/claude-haiku-4-5"),
		});

		render(() => <AiChatTab client={client} />);
		const select = (await screen.findByRole("combobox")) as HTMLSelectElement;
		expect(select.value).toBe("anthropic/claude-opus-5");

		fireEvent.change(select, { target: { value: "anthropic/claude-haiku-4-5" } });

		await vi.waitFor(() => expect(select.value).toBe("anthropic/claude-haiku-4-5"));
	});

	it("writes the chosen model through ego", async () => {
		const models = [model("anthropic/claude-opus-5"), model("anthropic/claude-haiku-4-5")];
		const client = fakeClient({
			providers: async () => snapshot([provider("anthropic", models)], "anthropic/claude-opus-5"),
			setDefaultModel: async (slug) => snapshot([provider("anthropic", models)], slug),
		});
		const spy = vi.spyOn(client, "setDefaultModel");

		render(() => <AiChatTab client={client} />);
		const select = (await screen.findByRole("combobox")) as HTMLSelectElement;
		fireEvent.change(select, { target: { value: "anthropic/claude-haiku-4-5" } });

		await vi.waitFor(() => expect(spy).toHaveBeenCalledWith("anthropic/claude-haiku-4-5"));
	});

	it("does not write when the selection did not move", async () => {
		const client = fakeClient({
			providers: async () =>
				snapshot([provider("anthropic", [model("anthropic/claude-opus-5")])], "anthropic/claude-opus-5"),
		});
		const spy = vi.spyOn(client, "setDefaultModel");

		render(() => <AiChatTab client={client} />);
		const select = (await screen.findByRole("combobox")) as HTMLSelectElement;
		fireEvent.change(select, { target: { value: "anthropic/claude-opus-5" } });

		expect(spy).not.toHaveBeenCalled();
	});

	it("offers no default until ego has one", async () => {
		const client = fakeClient({
			providers: async () => snapshot([provider("anthropic", [model("anthropic/claude-opus-5")])], null),
		});

		render(() => <AiChatTab client={client} />);

		// Without this option the picker would show the first model and imply ego
		// had chosen it.
		expect(await screen.findByRole("option", { name: "None — ego picks its own" })).toBeTruthy();
	});

	it("tells the four credential states apart", async () => {
		const client = fakeClient({
			providers: async () =>
				snapshot([
					provider("a-stored", [], { state: "stored", detail: "" }),
					provider("b-expired", [], { state: "expired", detail: "" }),
					provider("c-missing", [], { state: "missing" }),
					provider("d-unknown", [], { state: "unknown", detail: "" }),
				]),
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText("credential stored")).toBeTruthy();
		// Expired is not "log in again" — ego renews it by itself.
		expect(screen.getByText(/credential expired/)).toBeTruthy();
		expect(screen.getByText(/no credential/)).toBeTruthy();
		// A store doctor could not read is not an empty store.
		expect(screen.getByText(/could not read its credential store/)).toBeTruthy();
	});

	it("keeps doctor's own words about a credential", async () => {
		const client = fakeClient({
			providers: async () =>
				snapshot([
					provider("anthropic", [], { state: "unknown", detail: "the credential store is locked" }),
					// Nothing to report about a credential that is not there.
					provider("openrouter", [], { state: "missing" }),
				]),
		});

		render(() => <AiChatTab client={client} />);

		// The badge is our word for the state; this is ego's word for the case.
		expect(await screen.findByText("the credential store is locked")).toBeTruthy();
		expect(screen.getByText(/could not read its credential store/)).toBeTruthy();
	});

	it("explains what to do when ego is not configured", async () => {
		const client = fakeClient({
			providers: async () => {
				throw failure({ code: "notConfigured", message: "no ego executable is configured" });
			},
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText(/Set the ego executable in Settings → General/)).toBeTruthy();
		expect(screen.queryByRole("combobox")).toBeNull();
	});

	it("does not report a broken path as a missing configuration", async () => {
		const client = fakeClient({
			providers: async () => {
				throw failure({ code: "launchFailed", message: "no such file or directory (os error 2)" });
			},
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText(/could not be started\. Check the path in Settings → General/)).toBeTruthy();
		expect(screen.getByText(/os error 2/)).toBeTruthy();
		expect(screen.queryByText(/Set the ego executable/)).toBeNull();
	});

	// Catches: isEgoCliError rejecting the HttpRpcError transport.ts throws, so the browser falls back to the raw error string.
	it("shows the designed message when the HTTP transport carries a notConfigured error", async () => {
		const body = JSON.stringify(failure({ code: "notConfigured", message: "no ego executable is configured" }));
		const client = fakeClient({
			providers: async () => {
				throw new HttpRpcError("ego_providers", 409, body);
			},
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText(/Set the ego executable in Settings → General/)).toBeTruthy();
		expect(screen.queryByText(/HttpRpcError/)).toBeNull();
	});

	// Catches: the launchFailed branch missing the OS text when the failure arrives over HTTP.
	it("shows the launch failure and the OS text when the HTTP transport carries launchFailed", async () => {
		const body = JSON.stringify(failure({ code: "launchFailed", message: "no such file or directory (os error 2)" }));
		const client = fakeClient({
			providers: async () => {
				throw new HttpRpcError("ego_providers", 424, body);
			},
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText(/could not be started\. Check the path in Settings → General/)).toBeTruthy();
		expect(screen.getByText(/os error 2/)).toBeTruthy();
	});

	// Catches: an unrelated HTTP failure (no ego body) being dressed up as an ego refusal.
	it("keeps the raw error for an HTTP failure whose body is not an ego error", async () => {
		const client = fakeClient({
			providers: async () => {
				throw new HttpRpcError("ego_providers", 500, "boom");
			},
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText(/RPC ego_providers failed: 500 boom/)).toBeTruthy();
	});

	it("reports a failure with what ego printed", async () => {
		const client = fakeClient({
			providers: async () => {
				throw failure({
					message: "ego exited with 1",
					command: "ego models --json",
					stderr: "error: no provider is configured for 'openrouter'",
					exitCode: 1,
				});
			},
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText("ego exited with 1")).toBeTruthy();
		expect(screen.getByText("ego models --json")).toBeTruthy();
		// ego's own words, not a summary of them.
		expect(screen.getByText(/no provider is configured for 'openrouter'/)).toBeTruthy();
	});

	it("clears a failure once a later call succeeds", async () => {
		let firstCall = true;
		const client = fakeClient({
			providers: async () => {
				if (firstCall) {
					firstCall = false;
					throw failure({ message: "ego exited with 1", command: "ego doctor --json", exitCode: 1 });
				}
				return snapshot([provider("anthropic", [model("anthropic/claude-opus-5")])]);
			},
		});

		render(() => <AiChatTab client={client} />);
		await screen.findByText("ego exited with 1");

		fireEvent.click(screen.getByRole("button", { name: "Refresh from providers" }));

		expect(await screen.findByText("anthropic")).toBeTruthy();
		expect(screen.queryByText("ego exited with 1")).toBeNull();
	});

	it("keeps ego's reason for an unusable model, once per reason", async () => {
		const client = fakeClient({
			providers: async () =>
				snapshot([
					provider("openrouter", [
						model("openrouter/a", false, "the source is unreachable"),
						model("openrouter/b", false, "the source is unreachable"),
						model("openrouter/c"),
					]),
				]),
		});

		render(() => <AiChatTab client={client} />);

		// Two models, one cause: a source that is down says the same sentence
		// about every model it offers.
		expect(await screen.findAllByText("the source is unreachable")).toHaveLength(1);
		expect((screen.getByRole("option", { name: "a" }) as HTMLOptionElement).disabled).toBe(true);
		expect((screen.getByRole("option", { name: "c" }) as HTMLOptionElement).disabled).toBe(false);
	});

	it("reports a transport fault without attributing it to ego", async () => {
		const client = fakeClient({
			providers: async () => {
				throw new Error("the window has gone away");
			},
		});

		render(() => <AiChatTab client={client} />);

		expect(await screen.findByText(/the window has gone away/)).toBeTruthy();
		expect(screen.queryByText(/Set the ego executable/)).toBeNull();
	});
});

describe("AiChatTab — interactive login", () => {
	function twoProviders() {
		return fakeClient({
			providers: async () =>
				snapshot([
					provider("anthropic", [], { state: "missing" }),
					provider("kimi-coding", [], { state: "stored", detail: "" }),
				]),
		});
	}

	it("offers a Login action for each provider, named after it", async () => {
		render(() => <AiChatTab client={twoProviders()} startLogin={vi.fn()} />);

		expect(await screen.findByRole("button", { name: "Login to anthropic" })).toBeTruthy();
		expect(screen.getByRole("button", { name: "Login to kimi-coding" })).toBeTruthy();
	});

	it("starts the login for the clicked provider only, then leaves Settings for the terminal", async () => {
		const startLogin = vi.fn();
		const onClose = vi.fn();
		render(() => <AiChatTab client={twoProviders()} startLogin={startLogin} onClose={onClose} />);

		fireEvent.click(await screen.findByRole("button", { name: "Login to kimi-coding" }));

		expect(startLogin).toHaveBeenCalledTimes(1);
		expect(startLogin.mock.calls[0][0]).toBe("kimi-coding");
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it("asks ego doctor again when the login command has exited, without reaching the network", async () => {
		const refreshes: boolean[] = [];
		const client = fakeClient({
			providers: async (refresh: boolean) => {
				refreshes.push(refresh);
				return snapshot([provider("anthropic", [], { state: "missing" })]);
			},
		});
		let finished = () => {};
		render(() => (
			<AiChatTab
				client={client}
				startLogin={(_provider, onExit) => {
					finished = onExit;
					return "term-1";
				}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: "Login to anthropic" }));
		expect(refreshes).toEqual([false]);

		finished();
		await Promise.resolve();

		expect(refreshes).toEqual([false, false]);
	});

	it("says why nothing started when the login cannot be opened", async () => {
		const onClose = vi.fn();
		const startLogin = vi.fn(() => {
			throw new Error("The ego executable is not configured (Settings → General).");
		});
		render(() => <AiChatTab client={twoProviders()} startLogin={startLogin} onClose={onClose} />);

		fireEvent.click(await screen.findByRole("button", { name: "Login to anthropic" }));

		expect(await screen.findByText(/ego executable is not configured/)).toBeTruthy();
		expect(onClose).not.toHaveBeenCalled();
	});
});

// Boss decision 2026-09-24: ego is configured like MDKB, on General. The AI
// Chat page keeps only what ego itself stores: the default model and the
// providers. GeneralTabTools.test.tsx proves the picker's new home.
describe("AiChatTab — no ego executable control", () => {
	it("keeps executable configuration in General while naming the AI Chat page", async () => {
		const client = fakeClient();
		const { container } = render(() => <AiChatTab client={client} />);

		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.childNodes[0]?.textContent?.trim());
		expect(headings).toContain("Agent and model");
		expect(screen.getByRole("heading", { name: "AI Chat", level: 2 })).toBeTruthy();
		expect(headings).not.toContain("AI Chat");
		expect(screen.queryByRole("button", { name: "Select…" })).toBeNull();
		expect(screen.queryByText("ego executable")).toBeNull();
	});
});

beforeEach(() => {
	vi.spyOn(appLogger, "error").mockImplementation(() => {});
	vi.spyOn(appLogger, "warn").mockImplementation(() => {});
});

afterEach(async () => {
	cleanup();
	cleanupToasts();
	await Promise.resolve();
	// Provider transport-failure cases and the omitted perimeter/activity
	// responses have known diagnostics. Do not let an unrelated error pass.
	for (const [, message] of vi.mocked(appLogger.error).mock.calls) {
		expect([
			"ego command failed without an ego error",
			"ego perimeter request failed",
			"Failed to save activity",
		]).toContain(message);
	}
	for (const [source, message] of vi.mocked(appLogger.warn).mock.calls) {
		expect(source).toBe("config");
		expect(message).toBe("Settings reload failed");
	}
	vi.mocked(appLogger.warn).mockRestore();
	vi.mocked(appLogger.error).mockRestore();
});
