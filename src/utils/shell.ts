/**
 * Shell escaping utilities to prevent command injection
 */

import { isWindows } from "../platform";

/**
 * Escape a string for safe use in a shell command.
 * On POSIX (macOS/Linux): uses single quotes with escaped embedded single quotes.
 * On Windows: uses double quotes with escaped embedded double quotes and carets.
 */
export function escapeShellArg(arg: string): string {
  if (isWindows()) {
    // cmd.exe: wrap in double quotes, escape internal double quotes and special chars
    // Caret (^) is the cmd.exe escape character; double quotes need doubling
    const escaped = arg
      .replace(/"/g, '""')
      .replace(/([%^&<>|])/g, "^$1");
    return `"${escaped}"`;
  }
  // POSIX: replace single quotes with '\'' (end quote, escaped quote, start quote)
  return `'${arg.replace(/'/g, "'\\''")}'`;
}

/**
 * Validate a git branch name.
 * Git branch names cannot contain: ~, ^, :, \, *, ?, [, @{, .., spaces, or control characters.
 */
export function isValidBranchName(branch: string): boolean {
  if (!branch || branch.length === 0) return false;
  if (branch.length > 255) return false;

  // Check for forbidden patterns
  const forbidden = /[~^:\\*?\[\s\x00-\x1f\x7f]|@{|\.\.|\/\/|^\.|\/\.|\.lock$|^\/|\/$/;
  if (forbidden.test(branch)) return false;

  // Must not be empty after trimming
  if (branch.trim().length === 0) return false;

  return true;
}

/**
 * Validate a file path for use in git commands.
 * Ensures the path doesn't contain shell metacharacters that could cause injection.
 */
export function isValidPath(path: string): boolean {
  if (!path || path.length === 0) return false;

  // Check for null bytes
  if (path.includes('\0')) return false;

  // Common injection: path starts with shell operator
  if (/^[;&|`$]/.test(path)) return false;

  // Windows: reject cmd.exe metacharacters that could cause variable expansion or escaping
  if (isWindows()) {
    // % triggers env var expansion in cmd.exe (e.g. %PATH%)
    // ^ is the cmd.exe escape character
    if (/[%^]/.test(path)) return false;
  }

  return true;
}
