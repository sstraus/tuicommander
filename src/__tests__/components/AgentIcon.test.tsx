import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { AgentIcon, getAgentIconSvg } from "../../components/ui/AgentIcon";

const pathOf = (agent: Parameters<typeof AgentIcon>[0]["agent"], provider?: string | null) => {
	const { container } = render(() => <AgentIcon agent={agent} provider={provider} />);
	return container.querySelector("path")?.getAttribute("d") ?? null;
};

describe("AgentIcon for ego sessions", () => {
	// TEST-CHANGE: Boss 2026-10-16 wants ego, not the provider (replaces story 1736 expectations).
	// Catches: an ego session being mistaken for Codex or another provider's agent.
	it.each(["openai-codex", "openai", "anthropic", "gemini", "github-copilot", "xai", "openrouter"])(
		"ego on %s keeps the ego icon instead of the provider icon",
		(provider) => {
			const d = pathOf("ego", provider);
			expect(d).not.toBeNull();
			expect(d).toBe(pathOf("ego"));
			expect(d).not.toBe(pathOf("codex"));
		},
	);

	// Catches: an unknown or missing provider rendering nothing or another provider's icon.
	it("keeps the ego icon with an unknown or missing provider", () => {
		const generic = pathOf("ego", "something-else");
		expect(generic).not.toBeNull();
		expect(pathOf("ego")).toBe(generic);
		expect(pathOf("ego", null)).toBe(generic);
		expect(generic).not.toBe(pathOf("claude"));
	});

	// Catches: a provider leaking onto a non-ego agent.
	it.each(["codex", "claude", "gemini", "amp", "cursor", "droid", "opencode"] as const)(
		"ignores the provider for %s",
		(agent) => {
			expect(pathOf(agent, "openrouter")).toBe(pathOf(agent));
		},
	);

	// Catches: inline notification SVGs still identifying ego by its provider.
	it("keeps the ego icon in the inline svg too", () => {
		expect(getAgentIconSvg("ego", 14, "anthropic")).toBe(getAgentIconSvg("ego", 14));
		expect(getAgentIconSvg("ego", 14, "github-copilot")).toContain('fill-rule="evenodd"');
	});
});

// Catches: an ae ligature or square E replacing the Greek epsilon artwork.
describe("ego rounded epsilon artwork", () => {
	it("keeps the rounded Greek epsilon instead of an ae ligature or square E", () => {
		expect(pathOf("ego")).toMatchInlineSnapshot(
			`"M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 2a8 8 0 1 1 0 16 8 8 0 0 1 0-16zM16 8.2C15 7.4 13.8 7 12.3 7C9.6 7 8 8.3 8 10.2C8 11.1 8.5 11.8 9.4 12C8.3 12.4 7.7 13.2 7.7 14.3C7.7 16.1 9.5 17.2 12.3 17.2C13.9 17.2 15.2 16.7 16.2 15.8L15.1 14.5C14.4 15.2 13.5 15.5 12.3 15.5C10.6 15.5 9.6 15 9.6 14.1C9.6 13.2 10.4 12.8 12 12.8H14V11.2H12C10.6 11.2 9.8 10.9 9.8 10.2C9.8 9.3 10.7 8.7 12.3 8.7C13.4 8.7 14.2 9 14.9 9.5Z"`,
		);
	});

	// Catches: small sidebar/status icons losing the vector or using a font fallback.
	it.each([12, 14, 16, 22, 24])("preserves the epsilon vector at %ipx", (size) => {
		const { container } = render(() => <AgentIcon agent="ego" size={size} />);
		const svg = container.querySelector("svg");
		expect(svg?.getAttribute("width")).toBe(String(size));
		expect(svg?.getAttribute("height")).toBe(String(size));
		expect(svg?.getAttribute("fill")).toBe("currentColor");
		expect(svg?.querySelector("path")?.getAttribute("d")).toBe(pathOf("ego"));
		expect(container.querySelector("text, span")).toBeNull();
	});

	// Catches: notification markup retaining different artwork or filling the identity ring.
	it("uses the same ringed epsilon artwork in inline notifications", () => {
		const host = document.createElement("div");
		host.innerHTML = getAgentIconSvg("ego", 14) ?? "";
		expect(host.querySelector("path")?.getAttribute("d")).toBe(pathOf("ego"));
		expect(host.querySelector("path")?.getAttribute("fill-rule")).toBe("evenodd");
	});
});
