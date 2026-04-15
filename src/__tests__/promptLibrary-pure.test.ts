import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScopeAsync } from "./helpers/store";

// Mock invoke to handle the prompt template commands
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

/** Replicate extract_variables logic from Rust for test mocking */
function extractVarsLocal(content: string): string[] {
  const matches = content.match(/\{([^}]+)\}/g);
  if (!matches) return [];
  return [...new Set(matches.map((m) => m.slice(1, -1)))];
}

/** Replicate process_content logic from Rust for test mocking */
function processContentLocal(content: string, variables: Record<string, string>): string {
  let result = content;
  for (const [name, value] of Object.entries(variables)) {
    const pattern = new RegExp(`\\{${name}\\}`, "g");
    result = result.replace(pattern, value);
  }
  return result;
}

/** Replicate process_content_shell_safe logic from Rust for test mocking:
 * POSIX single-quoting on every substituted value. Windows would use a
 * different quoting rule, but this test file runs in Node/vitest where the
 * invoked backend is mocked so we assert the POSIX shape. */
function processContentShellSafeLocal(
  content: string,
  variables: Record<string, string>,
): string {
  let result = content;
  for (const [name, value] of Object.entries(variables)) {
    const quoted = `'${value.replace(/'/g, "'\\''")}'`;
    const pattern = new RegExp(`\\{${name}\\}`, "g");
    result = result.replace(pattern, quoted);
  }
  return result;
}

describe("promptLibrary pure functions", () => {
  let store: typeof import("../stores/promptLibrary").promptLibraryStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();

    // Re-mock after resetModules
    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: (...args: unknown[]) => mockInvoke(...args),
    }));

    mockInvoke.mockImplementation((cmd: string, args: Record<string, unknown>) => {
      switch (cmd) {
        case "extract_prompt_variables":
          return Promise.resolve(extractVarsLocal(args.content as string));
        case "process_prompt_content":
          return Promise.resolve(
            processContentLocal(args.content as string, args.variables as Record<string, string>)
          );
        case "process_prompt_content_shell_safe":
          return Promise.resolve(
            processContentShellSafeLocal(
              args.content as string,
              args.variables as Record<string, string>,
            )
          );
        case "save_prompt_library":
          return Promise.resolve(undefined);
        default:
          return Promise.resolve(undefined);
      }
    });

    const mod = await import("../stores/promptLibrary");
    store = mod.promptLibraryStore;
  });

  describe("extractVariables()", () => {
    it("extracts variables from content", async () => {
      const vars = await store.extractVariables("Hello {name}, welcome to {place}!");
      expect(vars).toEqual(["name", "place"]);
    });

    it("returns empty array for no variables", async () => {
      const vars = await store.extractVariables("Hello world!");
      expect(vars).toEqual([]);
    });

    it("deduplicates variables", async () => {
      const vars = await store.extractVariables("{name} and {name} again");
      expect(vars).toEqual(["name"]);
    });

    it("handles multiple variables correctly", async () => {
      const vars = await store.extractVariables("{a} and {b}");
      expect(vars).toEqual(["a", "b"]);
    });

    it("returns empty array for empty string", async () => {
      const vars = await store.extractVariables("");
      expect(vars).toEqual([]);
    });

    it("handles nested braces by matching greedily from first open brace", async () => {
      // The regex /\{([^}]+)\}/g matches from first { to first }
      // so "{{nested}}" captures "{nested" as the variable name
      const vars = await store.extractVariables("{{nested}}");
      expect(vars).toEqual(["{nested"]);
    });
  });

  describe("processContent()", () => {
    it("substitutes variables", async () => {
      await testInScopeAsync(async () => {
        const prompt = store.createPrompt({
          name: "test",
          content: "Hello {name}!",
          category: "custom",
          isFavorite: false,
        });
        const result = await store.processContent(prompt, { name: "World" });
        expect(result).toBe("Hello World!");
      });
    });

    it("substitutes multiple variables", async () => {
      await testInScopeAsync(async () => {
        const prompt = store.createPrompt({
          name: "test",
          content: "Hello {first} {last}!",
          category: "custom",
          isFavorite: false,
        });
        const result = await store.processContent(prompt, { first: "John", last: "Doe" });
        expect(result).toBe("Hello John Doe!");
      });
    });

    it("replaces all occurrences of same variable", async () => {
      await testInScopeAsync(async () => {
        const prompt = store.createPrompt({
          name: "test",
          content: "{x} + {x} = 2{x}",
          category: "custom",
          isFavorite: false,
        });
        const result = await store.processContent(prompt, { x: "5" });
        expect(result).toBe("5 + 5 = 25");
      });
    });

    it("leaves unmatched variables as-is", async () => {
      await testInScopeAsync(async () => {
        const prompt = store.createPrompt({
          name: "test",
          content: "Hello {name}, {unknown}!",
          category: "custom",
          isFavorite: false,
        });
        const result = await store.processContent(prompt, { name: "World" });
        expect(result).toBe("Hello World, {unknown}!");
      });
    });

    it("handles no variables in content", async () => {
      await testInScopeAsync(async () => {
        const prompt = store.createPrompt({
          name: "test",
          content: "No variables here",
          category: "custom",
          isFavorite: false,
        });
        const result = await store.processContent(prompt, {});
        expect(result).toBe("No variables here");
      });
    });

    it("routes shellSafe=true through process_prompt_content_shell_safe", async () => {
      await testInScopeAsync(async () => {
        const prompt = store.createPrompt({
          name: "test",
          content: "git checkout {branch}",
          category: "custom",
          isFavorite: false,
        });
        const baseline = mockInvoke.mock.calls.length;
        const result = await store.processContent(
          prompt,
          { branch: "main'; rm -rf ~; echo '" },
          { shellSafe: true },
        );
        expect(result).toBe("git checkout 'main'\\''; rm -rf ~; echo '\\'''");
        // Confirm this specific call went to the shell_safe endpoint, not the
        // plain one. Earlier setup calls (save_prompt_library etc.) are ignored
        // by snapshotting the invoke count before processContent.
        const newCalls = mockInvoke.mock.calls.slice(baseline).map((c) => c[0]);
        expect(newCalls).toEqual(["process_prompt_content_shell_safe"]);
      });
    });

    it("defaults to plain process_prompt_content when shellSafe is not set", async () => {
      await testInScopeAsync(async () => {
        const prompt = store.createPrompt({
          name: "test",
          content: "Hello {name}",
          category: "custom",
          isFavorite: false,
        });
        const baseline = mockInvoke.mock.calls.length;
        const result = await store.processContent(prompt, { name: "World" });
        expect(result).toBe("Hello World");
        const newCalls = mockInvoke.mock.calls.slice(baseline).map((c) => c[0]);
        expect(newCalls).toEqual(["process_prompt_content"]);
      });
    });
  });
});
