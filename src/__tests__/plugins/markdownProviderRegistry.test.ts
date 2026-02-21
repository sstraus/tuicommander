import { describe, it, expect, beforeEach } from "vitest";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import type { MarkdownProvider } from "../../plugins/types";

const syncProvider = (content: string): MarkdownProvider => ({
  provideContent: () => content,
});

const asyncProvider = (content: string | null): MarkdownProvider => ({
  provideContent: () => Promise.resolve(content),
});

const nullProvider: MarkdownProvider = {
  provideContent: () => null,
};

describe("markdownProviderRegistry", () => {
  beforeEach(() => {
    markdownProviderRegistry.clear();
  });

  // -------------------------------------------------------------------------
  // Registration
  // -------------------------------------------------------------------------
  describe("register", () => {
    it("registers a provider and returns a Disposable", () => {
      const d = markdownProviderRegistry.register("plan", syncProvider("# Plan"));
      expect(d).toBeDefined();
      expect(typeof d.dispose).toBe("function");
      d.dispose();
    });

    it("dispose removes the provider so resolve returns null", async () => {
      const d = markdownProviderRegistry.register("plan", syncProvider("hello"));
      d.dispose();
      const result = await markdownProviderRegistry.resolve("plan:file?path=/foo.md");
      expect(result).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // Resolve — sync providers
  // -------------------------------------------------------------------------
  describe("resolve with sync provider", () => {
    it("returns content from registered scheme", async () => {
      markdownProviderRegistry.register("plan", syncProvider("# Plan Content"));
      const result = await markdownProviderRegistry.resolve("plan:file?path=/foo.md");
      expect(result).toBe("# Plan Content");
    });

    it("returns null for unknown scheme", async () => {
      const result = await markdownProviderRegistry.resolve("unknown:foo");
      expect(result).toBeNull();
    });

    it("returns null when provider returns null", async () => {
      markdownProviderRegistry.register("plan", nullProvider);
      const result = await markdownProviderRegistry.resolve("plan:whatever");
      expect(result).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // Resolve — async providers
  // -------------------------------------------------------------------------
  describe("resolve with async provider", () => {
    it("awaits async provider and returns content", async () => {
      markdownProviderRegistry.register("stories", asyncProvider("# Stories"));
      const result = await markdownProviderRegistry.resolve("stories:detail?id=042");
      expect(result).toBe("# Stories");
    });

    it("awaits async provider returning null", async () => {
      markdownProviderRegistry.register("stories", asyncProvider(null));
      const result = await markdownProviderRegistry.resolve("stories:detail?id=042");
      expect(result).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // URI parsing
  // -------------------------------------------------------------------------
  describe("URI parsing", () => {
    it("routes by scheme, ignoring query params", async () => {
      markdownProviderRegistry.register("plan", syncProvider("Plan"));
      markdownProviderRegistry.register("stories", syncProvider("Stories"));
      expect(await markdownProviderRegistry.resolve("plan:file?path=/foo.md")).toBe("Plan");
      expect(await markdownProviderRegistry.resolve("stories:detail?id=1")).toBe("Stories");
    });

    it("passes the full URI to the provider", async () => {
      let receivedUri: URL | null = null;
      const spy: MarkdownProvider = {
        provideContent: (uri: URL) => {
          receivedUri = uri;
          return "ok";
        },
      };
      markdownProviderRegistry.register("plan", spy);
      await markdownProviderRegistry.resolve("plan:file?path=/foo/bar.md");
      expect(receivedUri).not.toBeNull();
      expect((receivedUri as unknown as URL).searchParams.get("path")).toBe("/foo/bar.md");
    });

    it("returns null for malformed URI", async () => {
      const result = await markdownProviderRegistry.resolve("not a uri");
      expect(result).toBeNull();
    });

    it("returns null for URI without scheme separator", async () => {
      const result = await markdownProviderRegistry.resolve("justtext");
      expect(result).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // Multiple schemes
  // -------------------------------------------------------------------------
  describe("multiple schemes", () => {
    it("routes two different schemes independently", async () => {
      markdownProviderRegistry.register("plan", syncProvider("plan content"));
      markdownProviderRegistry.register("mem", syncProvider("memory content"));
      expect(await markdownProviderRegistry.resolve("plan:x")).toBe("plan content");
      expect(await markdownProviderRegistry.resolve("mem:x")).toBe("memory content");
    });

    it("last registered provider for same scheme wins", async () => {
      markdownProviderRegistry.register("plan", syncProvider("first"));
      markdownProviderRegistry.register("plan", syncProvider("second"));
      expect(await markdownProviderRegistry.resolve("plan:x")).toBe("second");
    });

    it("disposing second registration restores first", async () => {
      markdownProviderRegistry.register("plan", syncProvider("first"));
      const d = markdownProviderRegistry.register("plan", syncProvider("second"));
      d.dispose();
      expect(await markdownProviderRegistry.resolve("plan:x")).toBe("first");
    });
  });
});
