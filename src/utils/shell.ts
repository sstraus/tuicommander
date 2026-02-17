/**
 * Shell escaping utilities to prevent command injection
 */

/**
 * Escape a string for safe use in a shell command.
 * Uses single quotes and escapes embedded single quotes.
 */
export function escapeShellArg(arg: string): string {
  // Replace single quotes with '\'' (end quote, escaped quote, start quote)
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

  // Path should start with / or alphanumeric (not shell special chars)
  if (/^[;&|`$]/.test(path)) return false;

  return true;
}
