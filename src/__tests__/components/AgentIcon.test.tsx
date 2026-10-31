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

// Catches: the hand-drawn square E replacing the joined lowercase ae artwork.
describe("ego rounded ae artwork", () => {
	it("keeps the rounded ae ligature artwork instead of a square E", () => {
		expect(pathOf("ego")).toMatchInlineSnapshot(
			`"M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 2a8 8 0 1 1 0 16 8 8 0 0 1 0-16zM9 8C7.5 8 6.5 8.6 6.2 9.8L7.7 10.2C7.9 9.6 8.3 9.3 9 9.3C10 9.3 10.5 9.8 10.5 10.6V11H9C7 11 6 11.9 6 13.4C6 15 7.2 16 8.8 16C10 16 10.9 15.5 11.5 14.7C12.2 15.6 13.3 16 14.6 16C16.2 16 17.4 15.3 18 14L16.6 13.4C16.2 14.2 15.6 14.6 14.6 14.6C13.2 14.6 12.4 13.8 12.3 12.6H18V11.9C18 9.5 16.7 8 14.6 8C13.3 8 12.3 8.5 11.6 9.4C11.1 8.5 10.2 8 9 8ZM10.5 12.3H9C8 12.3 7.5 12.7 7.5 13.4C7.5 14.2 8 14.6 8.8 14.6C9.9 14.6 10.5 13.8 10.5 12.8ZM12.3 11.3H16.4C16.3 10 15.7 9.4 14.5 9.4C13.3 9.4 12.5 10.1 12.3 11.3Z"`,
		);
	});

	// Catches: small sidebar/status icons losing the vector or using a font fallback.
	it.each([12, 14, 16, 22, 24])("preserves the ae vector at %ipx", (size) => {
		const { container } = render(() => <AgentIcon agent="ego" size={size} />);
		const svg = container.querySelector("svg");
		expect(svg?.getAttribute("width")).toBe(String(size));
		expect(svg?.getAttribute("height")).toBe(String(size));
		expect(svg?.getAttribute("fill")).toBe("currentColor");
		expect(svg?.querySelector("path")?.getAttribute("d")).toBe(pathOf("ego"));
		expect(container.querySelector("text, span")).toBeNull();
	});

	// Catches: notification markup retaining different artwork or filling the ae counters.
	it("uses the same hollow ae artwork in inline notifications", () => {
		const host = document.createElement("div");
		host.innerHTML = getAgentIconSvg("ego", 14) ?? "";
		expect(host.querySelector("path")?.getAttribute("d")).toBe(pathOf("ego"));
		expect(host.querySelector("path")?.getAttribute("fill-rule")).toBe("evenodd");
	});
});
