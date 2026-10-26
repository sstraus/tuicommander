import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export interface ChatAppearance {
	mobile: boolean;
	theme: string;
	terminal: { color: string; foreground: string };
	acp: { color: string; foreground: string };
	lists: {
		tag: string;
		border: number;
		padding: number;
		font: number;
		ruleRight: number;
		itemLeft: number;
		textLeft: number;
		marker: string;
	}[];
	unmarkedBorder: string;
}

/** Uses the approved named-tab wrapper: no Chrome dump-dom dependency or shared-app access. */
export function captureChatReadability(baseUrl: string, directory: string, phase: string): ChatAppearance[] {
	mkdirSync(join(directory, "ab-empty"), { recursive: true });
	const wrapper =
		process.env.TUIC_CHAT_BROWSER_WRAPPER ??
		join(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
	const env = {
		...process.env,
		AB_SESSION: process.env.AB_SESSION ?? "tuic2159",
		AB_AGENT_BROWSER_DIR: join(directory, "ab-empty"),
	};
	const run = (...args: string[]) =>
		execFileSync("perl", ["-e", "alarm 40; exec @ARGV", wrapper, ...args], {
			env,
			encoding: "utf8",
			timeout: 45000,
			stdio: ["ignore", "pipe", "pipe"],
		});
	const appearances: ChatAppearance[] = [];
	try {
		for (const mobile of [false, true]) {
			for (const theme of ["dark", "light"]) {
				const name = `${phase}-${mobile ? "390" : "1400"}-${theme}`;
				run("mac-chrome", `${baseUrl}/src/__tests__/browser/chatReadability.html?theme=${theme}&mobile=${mobile}`);
				run("--", "set", "viewport", mobile ? "390" : "1400", "1100");
				run("--", "wait", "--fn", "typeof window.chatReadability === 'function'");
				const appearance = JSON.parse(run("--", "eval", "window.chatReadability()")) as ChatAppearance;
				writeFileSync(join(directory, `${name}.json`), JSON.stringify(appearance, null, 2));
				run("--", "screenshot", join(directory, `${name}.png`));
				appearances.push(appearance);
			}
		}
	} finally {
		run("--", "tab", "close");
	}
	return appearances;
}
