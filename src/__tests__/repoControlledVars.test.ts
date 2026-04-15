import { describe, it, expect } from "vitest";
import { repoControlledVarsInContent } from "../components/SettingsPanel/tabs/SmartPromptsTab";

describe("repoControlledVarsInContent", () => {
  it("detects a single repo-controlled variable", () => {
    expect(repoControlledVarsInContent("git checkout {branch}")).toEqual(["branch"]);
  });

  it("detects multiple repo-controlled variables and returns them sorted & unique", () => {
    const content = "echo {pr_title} on {branch}; diff was {diff}; {branch} again";
    expect(repoControlledVarsInContent(content)).toEqual(["branch", "diff", "pr_title"]);
  });

  it("ignores non-repo-controlled variables", () => {
    // `name` is not a repo-controlled variable — it's caller-supplied.
    expect(repoControlledVarsInContent("hello {name}")).toEqual([]);
  });

  it("returns empty array when content has no variables", () => {
    expect(repoControlledVarsInContent("plain text")).toEqual([]);
    expect(repoControlledVarsInContent("")).toEqual([]);
  });

  it("handles mixed repo-controlled and custom variables", () => {
    expect(repoControlledVarsInContent("{custom} on {branch}")).toEqual(["branch"]);
  });

  it("covers the full repo-controlled set", () => {
    // Smoke-check the most sensitive ones for shell-injection (user-writable
    // via crafted branches/PRs).
    for (const v of [
      "branch",
      "commit_log",
      "pr_title",
      "pr_author",
      "remote_url",
      "changed_files",
    ]) {
      expect(repoControlledVarsInContent(`prefix {${v}} suffix`)).toContain(v);
    }
  });
});
