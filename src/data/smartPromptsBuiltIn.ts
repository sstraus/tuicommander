import type { SavedPrompt, SmartPlacement } from "../stores/promptLibrary";

/** Shared defaults for all built-in smart prompts */
function builtin(
  id: string,
  name: string,
  icon: string,
  placement: SmartPlacement[],
  content: string,
  tags: string[],
  opts: Partial<SavedPrompt> = {},
): SavedPrompt {
  return {
    id,
    name,
    content,
    icon,
    placement,
    tags: ["smart", ...tags],
    category: "custom",
    isFavorite: false,
    builtIn: true,
    builtInVersion: 1,
    autoExecute: true,
    requiresIdle: true,
    executionMode: "inject",
    enabled: true,
    createdAt: 0,
    updatedAt: 0,
    ...opts,
  };
}

export const SMART_PROMPTS_BUILTIN: SavedPrompt[] = [
  // ── Git & Commit ────────────────────────────────────────────────
  builtin(
    "smart-commit",
    "Smart Commit",
    "git-commit",
    ["toolbar", "git-changes"],
    "Look at the currently staged changes and create a commit with a good conventional commit message. If nothing is staged, stage the relevant changes first.",
    ["git"],
  ),
  builtin(
    "smart-commit-push",
    "Commit & Push",
    "git-push",
    ["toolbar"],
    "Stage all changes, create a commit with a conventional commit message, and push to origin/{branch}.",
    ["git"],
  ),
  builtin(
    "smart-amend",
    "Amend Commit",
    "git-amend",
    ["git-changes"],
    "Amend the last commit with the currently staged changes. Update the commit message if the scope changed.",
    ["git"],
  ),
  builtin(
    "smart-commit-msg",
    "Generate Commit Message",
    "clipboard",
    ["git-changes"],
    "Generate a conventional commit message for these staged changes. Output ONLY the commit message, nothing else.\n\n{staged_diff}",
    ["git"],
    { executionMode: "headless", outputTarget: "clipboard" },
  ),

  // ── Code Review ─────────────────────────────────────────────────
  builtin(
    "smart-review",
    "Review Changes",
    "magnifier",
    ["toolbar", "git-changes"],
    "Review the uncommitted changes in this repository for bugs, security issues, performance problems, and code quality. Be specific with file names and line numbers.\n\nChanged files:\n{changed_files}",
    ["review"],
  ),
  builtin(
    "smart-review-staged",
    "Review Staged",
    "check-diff",
    ["git-changes"],
    "Review only the staged changes for bugs, security issues, and code quality:\n\n{staged_diff}",
    ["review"],
  ),
  builtin(
    "smart-review-pr",
    "Review PR",
    "pr-review",
    ["pr-popover"],
    'Review PR #{pr_number} "{pr_title}". Checkout the branch if needed, review all changes, and provide detailed feedback on code quality, bugs, security, and improvements.',
    ["review"],
  ),
  builtin(
    "smart-review-comments",
    "Address Review Comments",
    "chat",
    ["pr-popover"],
    "Check the review comments on PR #{pr_number} and address each one. For each comment, either make the requested change or explain why you disagree.",
    ["review"],
  ),

  // ── Pull Requests ───────────────────────────────────────────────
  builtin(
    "smart-create-pr",
    "Create PR",
    "pr-open",
    ["toolbar", "git-branches"],
    "Create a GitHub pull request for branch {branch} targeting {base_branch}. Generate a descriptive title and comprehensive description based on the commits:\n\n{commit_log}\n\nUse `gh pr create`.",
    ["pr"],
  ),
  builtin(
    "smart-update-pr-desc",
    "Update PR Description",
    "edit",
    ["pr-popover"],
    "Update the description of PR #{pr_number} to accurately reflect the current state of changes. Use `gh pr edit`.",
    ["pr"],
  ),
  builtin(
    "smart-pr-description",
    "Generate PR Description",
    "clipboard",
    ["pr-popover"],
    "Generate a pull request title and description for branch {branch} targeting {base_branch}. Format as:\n\nTitle: <title>\n\n<description body>\n\nBased on commits:\n{commit_log}",
    ["pr"],
    { executionMode: "headless", outputTarget: "clipboard" },
  ),

  // ── Merge & Conflicts ──────────────────────────────────────────
  builtin(
    "smart-resolve-conflicts",
    "Resolve Conflicts",
    "merge",
    ["toolbar", "pr-popover"],
    "Resolve the merge conflicts in this repository. The conflicting files are:\n{conflict_files}\n\nFor each file, analyze both sides of the conflict, choose the best resolution that preserves intent from both branches, and stage the resolved files.",
    ["merge"],
  ),
  builtin(
    "smart-merge-main",
    "Merge Main Into Branch",
    "git-merge",
    ["toolbar", "git-branches"],
    "Merge the latest {base_branch} into {branch}. If there are conflicts, resolve them thoughtfully — favor the feature branch intent while incorporating upstream changes. Stage and commit the merge.",
    ["merge"],
  ),
  builtin(
    "smart-rebase-main",
    "Rebase on Main",
    "git-rebase",
    ["git-branches"],
    "Rebase {branch} onto the latest {base_branch}. Resolve any conflicts that arise, then force-push if needed.",
    ["merge"],
  ),

  // ── CI & Quality ────────────────────────────────────────────────
  builtin(
    "smart-fix-ci",
    "Fix CI Failures",
    "ci-fix",
    ["pr-popover"],
    "PR #{pr_number} has failing CI checks: {pr_checks}\n\nInvestigate the failures, identify the root cause, and fix them. Run the failing tests locally to verify before pushing.",
    ["ci"],
  ),
  builtin(
    "smart-fix-lint",
    "Fix Lint Issues",
    "lint",
    ["toolbar"],
    "Run the linter/formatter for this project and fix all issues. Don't change logic — only fix formatting, style, and lint violations.",
    ["ci"],
  ),
  builtin(
    "smart-write-tests",
    "Write Tests",
    "test",
    ["toolbar", "git-changes"],
    "Write comprehensive tests for the changes in this branch. Focus on:\n\nChanged files:\n{changed_files}\n\nCover happy paths, edge cases, and error conditions. Follow the existing test patterns in the project.",
    ["ci"],
  ),
  builtin(
    "smart-run-tests",
    "Run & Fix Tests",
    "test-run",
    ["toolbar"],
    "Run the test suite. If any tests fail, investigate the root cause and fix them. Report what was wrong and what you fixed.",
    ["ci"],
  ),

  // ── Investigation & Context ─────────────────────────────────────
  builtin(
    "smart-investigate-issue",
    "Investigate Issue",
    "search",
    ["toolbar"],
    "Investigate GitHub issue #{issue_number}. Read the issue, understand the expected vs actual behavior, explore relevant code, identify the root cause, and propose a solution with specific file paths.",
    ["investigation"],
  ),
  builtin(
    "smart-what-changed",
    "What Changed?",
    "history",
    ["toolbar"],
    "Summarize what changed on this branch since it diverged from {base_branch}. Show:\n- Overview of changes\n- Files modified and why\n- Any potential issues\n\nRecent commits:\n{commit_log}",
    ["investigation"],
  ),
  builtin(
    "smart-summarize-branch",
    "Summarize Branch",
    "branch-info",
    ["toolbar", "git-branches"],
    "Summarize what branch {branch} does based on these commits:\n{commit_log}\n\nProvide a 2-3 sentence summary.",
    ["investigation"],
    { executionMode: "headless", outputTarget: "toast" },
  ),
  builtin(
    "smart-explain-diff",
    "Explain Changes",
    "explain",
    ["git-changes"],
    "Explain the current uncommitted changes in plain language. What do they do, and why might they have been made?\n\nChanged files:\n{changed_files}",
    ["investigation"],
  ),

  // ── Code Operations ─────────────────────────────────────────────
  builtin(
    "smart-refactor",
    "Suggest Refactoring",
    "refactor",
    ["toolbar"],
    "Analyze the recent changes and suggest refactoring opportunities. Focus on reducing duplication, improving naming, and simplifying complex logic.\n\nChanged files:\n{changed_files}",
    ["code"],
  ),
  builtin(
    "smart-security",
    "Security Audit",
    "shield",
    ["toolbar"],
    "Perform a security review of the uncommitted changes. Check for:\n- Injection vulnerabilities (SQL, command, XSS)\n- Credential exposure\n- Unsafe deserialization\n- Missing input validation\n- Dependency vulnerabilities\n\n{changed_files}",
    ["code"],
  ),
];
