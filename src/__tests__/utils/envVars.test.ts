import { describe, it, expect } from "vitest";
import { findDuplicateEnvKeys, buildEnvFromEntries } from "../../utils/envVars";

describe("findDuplicateEnvKeys()", () => {
  it("returns empty array for unique keys", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "BAR", value: "2" },
    ];
    expect(findDuplicateEnvKeys(entries)).toEqual([]);
  });

  it("returns duplicated keys", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "BAR", value: "2" },
      { key: "FOO", value: "3" },
    ];
    expect(findDuplicateEnvKeys(entries)).toEqual(["FOO"]);
  });

  it("is case-sensitive", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "foo", value: "2" },
    ];
    expect(findDuplicateEnvKeys(entries)).toEqual([]);
  });

  it("treats keys as duplicate after trimming whitespace", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "  FOO  ", value: "2" },
    ];
    expect(findDuplicateEnvKeys(entries)).toEqual(["FOO"]);
  });

  it("ignores empty and whitespace-only keys", () => {
    const entries = [
      { key: "", value: "1" },
      { key: "   ", value: "2" },
      { key: "", value: "3" },
    ];
    expect(findDuplicateEnvKeys(entries)).toEqual([]);
  });

  it("reports each duplicated key only once", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "FOO", value: "2" },
      { key: "FOO", value: "3" },
    ];
    expect(findDuplicateEnvKeys(entries)).toEqual(["FOO"]);
  });

  it("reports multiple distinct duplicates", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "BAR", value: "2" },
      { key: "FOO", value: "3" },
      { key: "BAR", value: "4" },
    ];
    expect(findDuplicateEnvKeys(entries).sort()).toEqual(["BAR", "FOO"]);
  });
});

describe("buildEnvFromEntries()", () => {
  it("builds a Record from unique entries", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "BAR", value: "2" },
    ];
    expect(buildEnvFromEntries(entries)).toEqual({ FOO: "1", BAR: "2" });
  });

  it("trims keys in the resulting Record", () => {
    const entries = [
      { key: "  FOO  ", value: "1" },
    ];
    expect(buildEnvFromEntries(entries)).toEqual({ FOO: "1" });
  });

  it("preserves value whitespace", () => {
    const entries = [
      { key: "FOO", value: "  spaced  " },
    ];
    expect(buildEnvFromEntries(entries)).toEqual({ FOO: "  spaced  " });
  });

  it("filters out empty/whitespace-only keys", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "", value: "2" },
      { key: "   ", value: "3" },
    ];
    expect(buildEnvFromEntries(entries)).toEqual({ FOO: "1" });
  });

  it("throws on duplicate keys with descriptive message", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "FOO", value: "2" },
    ];
    expect(() => buildEnvFromEntries(entries)).toThrow(/Duplicate env keys.*FOO/);
  });

  it("lists all duplicates in the error message", () => {
    const entries = [
      { key: "FOO", value: "1" },
      { key: "FOO", value: "2" },
      { key: "BAR", value: "3" },
      { key: "BAR", value: "4" },
    ];
    expect(() => buildEnvFromEntries(entries)).toThrow(/FOO.*BAR|BAR.*FOO/);
  });
});
