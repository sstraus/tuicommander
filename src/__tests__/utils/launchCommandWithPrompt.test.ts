import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";
import { AGENT_TYPES } from "../../agents";
import { launchCommandWithPrompt } from "../../utils/agentLaunchCommand";

/** What a POSIX shell hands the program for the quoted tail of `command`: the independent oracle for quoting. */
function argvAfter(command: string, head: string): string {
	const tail = command.slice(`${head} -- `.length);
	return execFileSync("sh", ["-c", `printf %s ${tail}`]).toString();
}

// Each agent below was checked against its own `--help` (a trailing [PROMPT]/query starts the interactive session
// with that message) and run as `<cli> -- --version`, which must not print a version. pi lists [messages] but its
// parser has no `--` handling (`--` is an unknown flag that takes the next word as its value), so it is typed.
const WITH_ARGV = ["claude", "codex", "agy", "grok"] as const;

describe("launchCommandWithPrompt", () => {
	// Catches: an agent that takes no positional prompt being given one (opencode treats it as a project path).
	it("only_agents_whose_help_lists_an_interactive_prompt_argument_get_one", () => {
		const supported = AGENT_TYPES.filter((type) => launchCommandWithPrompt(type, type, "x", "posix") !== null);
		expect(supported).toEqual([...WITH_ARGV].sort((a, b) => AGENT_TYPES.indexOf(a) - AGENT_TYPES.indexOf(b)));
	});

	// Catches: quoting that lets a quote, substitution, backtick or newline alter the command line.
	it.each(WITH_ARGV)("%s_receives_the_prompt_verbatim", (type) => {
		const prompt = 'line one\nit\'s $(touch /tmp/pwned) `id` "q" \\ ; & | !';
		const command = launchCommandWithPrompt(type, `${type} --flag`, prompt, "posix");
		expect(command).not.toBeNull();
		expect(argvAfter(command as string, `${type} --flag`)).toBe(prompt);
	});

	// Catches: a multi-line prompt quoted for cmd/PowerShell, whose quoting cannot carry a newline.
	it("windows_native_shells_fall_back_to_typing", () => {
		expect(launchCommandWithPrompt("claude", "claude", "a\nb", "windows-native")).toBeNull();
	});
});

// Catches: a trailing Claude --add-dir consumes the task brief as another directory instead of a prompt.
it("separates_the_initial_prompt_from_a_trailing_variadic_add_dir", () => {
	expect(launchCommandWithPrompt("claude", "claude --add-dir /repo/shared", "Fix issue 125", "posix")).toBe(
		"claude --add-dir /repo/shared -- 'Fix issue 125'",
	);
});
