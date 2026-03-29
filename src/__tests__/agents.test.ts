import { describe, it, expect } from "vitest";
import { AGENTS, AGENT_TYPES, MCP_SUPPORT, AGENT_DISPLAY, type AgentType } from "../agents";

describe("agents", () => {
  /** Agent types that are not real CLI agents and should be excluded from UI agent lists */
  const NON_CLI_TYPES: AgentType[] = ["git", "api"];

  it("AGENT_TYPES includes 'api'", () => {
    expect(AGENT_TYPES).toContain("api");
  });

  it("AGENTS has an entry for 'api'", () => {
    expect(AGENTS.api).toBeDefined();
    expect(AGENTS.api.name).toBe("External API");
  });

  it("'api' agent has no binary (it's not a CLI)", () => {
    expect(AGENTS.api.binary).toBe("");
  });

  it("MCP_SUPPORT excludes 'api'", () => {
    expect(MCP_SUPPORT.api).toBe(false);
  });

  it("AGENT_DISPLAY has an entry for 'api'", () => {
    expect(AGENT_DISPLAY.api).toBeDefined();
  });

  it("all AGENT_TYPES have corresponding entries in AGENTS, MCP_SUPPORT, and AGENT_DISPLAY", () => {
    for (const type of AGENT_TYPES) {
      expect(AGENTS[type], `AGENTS.${type}`).toBeDefined();
      expect(MCP_SUPPORT[type], `MCP_SUPPORT.${type}`).toBeDefined();
      expect(AGENT_DISPLAY[type], `AGENT_DISPLAY.${type}`).toBeDefined();
    }
  });

  it("non-CLI types (git, api) have no defaultHeadlessTemplate or empty string", () => {
    for (const type of NON_CLI_TYPES) {
      const template = AGENTS[type].defaultHeadlessTemplate;
      expect(!template, `${type} should have no headless template`).toBe(true);
    }
  });
});
