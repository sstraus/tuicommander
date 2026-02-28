import { describe, it, expect } from "vitest";
import { parseOsc7Url } from "../../utils/osc7";

describe("parseOsc7Url", () => {
  it("parses a standard file:// URL with hostname", () => {
    expect(parseOsc7Url("file://myhost/Users/john/project")).toBe(
      "/Users/john/project",
    );
  });

  it("parses a file:// URL with empty hostname", () => {
    expect(parseOsc7Url("file:///Users/john/project")).toBe(
      "/Users/john/project",
    );
  });

  it("parses a file:// URL with localhost", () => {
    expect(parseOsc7Url("file://localhost/tmp/test")).toBe("/tmp/test");
  });

  it("decodes percent-encoded paths", () => {
    expect(parseOsc7Url("file:///Users/john/my%20project")).toBe(
      "/Users/john/my project",
    );
    expect(parseOsc7Url("file:///tmp/path%23with%23hashes")).toBe(
      "/tmp/path#with#hashes",
    );
  });

  it("returns null for non-file schemes", () => {
    expect(parseOsc7Url("http://example.com/path")).toBeNull();
    expect(parseOsc7Url("https://example.com/path")).toBeNull();
    expect(parseOsc7Url("ftp://server/path")).toBeNull();
  });

  it("returns null for malformed URLs", () => {
    expect(parseOsc7Url("")).toBeNull();
    expect(parseOsc7Url("not a url")).toBeNull();
  });

  it("strips trailing slashes from paths", () => {
    expect(parseOsc7Url("file:///Users/john/project/")).toBe(
      "/Users/john/project",
    );
  });

  it("preserves root path", () => {
    expect(parseOsc7Url("file:///")).toBe("/");
  });
});
