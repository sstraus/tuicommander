// @vitest-environment node
import { type ChildProcess, execFileSync, spawn, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

type Box = { top: number; bottom: number; left: number; right: number; height: number; width: number };
type Sample = {
	text: string;
	focus: boolean;
	actualFocus: boolean;
	focusVisible: boolean;
	mobile: boolean;
	busy: boolean;
	stop: Box | null;
	editor: Box;
	send: Box;
	header: Box;
	footer: Box;
	focusStyle: { outline: string; shadow: string };
};
const directory = process.env.TUIC_COMPOSER_GEOMETRY_DIR;
const phase = process.env.TUIC_COMPOSER_GEOMETRY_PHASE ?? "after";
// Opt-in: happy-dom cannot measure CSS. This mounts the real component in worktree Vite.
describe.skipIf(!directory)("AI Chat composer fixed actions", () => {
	let server: ChildProcess;
	const samples: Sample[] = [];
	const mobile: Sample[] = [];
	const wrapper = join(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
	let cdpPort = "";
	let daemon: string | undefined;
	const run = (...args: string[]) => {
		const result = spawnSync("perl", ["-e", "alarm 40; exec @ARGV", wrapper, ...args], {
			encoding: "utf8",
			timeout: 45000,
			env: {
				...process.env,
				AB_SESSION: "tuic-compose-buttons-2588",
				AB_AGENT_BROWSER_DIR: join(directory!, "ab-empty"),
			},
			stdio: ["ignore", "pipe", "pipe"],
		});
		const pid = result.stderr?.match(/session=tuic-compose-buttons-2588 Chrome=(\d+)/)?.[1];
		if (pid) daemon = `ab-shared-tuic-compose-buttons-2588-${pid}`;
		if (result.status !== 0) throw new Error(result.stderr || String(result.error));
		return result.stdout;
	};
	beforeAll(async () => {
		mkdirSync(join(directory!, "ab-empty"), { recursive: true });
		// Separate event loop: synchronous browser commands must not block Vite's HTTP responses.
		server = spawn(
			process.execPath,
			[
				"--input-type=module",
				"-e",
				`
			import { createServer } from 'vite'; import solid from 'vite-plugin-solid';
			const server = await createServer({ configFile:false, plugins:[solid()], logLevel:'error',
			 cacheDir:'.tmp/composer-geometry-vite',
			 define:{__APP_VERSION__:JSON.stringify('geometry'),__BUILD_GIT_HASH__:JSON.stringify('geometry')},
			 server:{host:'127.0.0.1',port:0} });
			await server.listen(); console.log('FIXTURE_PORT='+server.httpServer.address().port);
          let socket;
          process.on('message', async ({cdpPort,url}) => {
            try {
              socket?.close();
              const pages = await (await fetch('http://127.0.0.1:'+cdpPort+'/json/list')).json();
              const page = pages.find(page=>page.url.startsWith(url));
              if(!page) throw new Error('Missing owned fixture target');
              socket = new WebSocket(page.webSocketDebuggerUrl);
              await new Promise((ok,fail)=>{socket.addEventListener('open',ok,{once:true});socket.addEventListener('error',fail,{once:true});});
              await new Promise((ok,fail)=>{
                socket.addEventListener('message',event=>{const answer=JSON.parse(event.data);answer.error?fail(new Error(JSON.stringify(answer.error))):ok();},{once:true});
                socket.send(JSON.stringify({id:1,method:'Emulation.setFocusEmulationEnabled',params:{enabled:true}}));
              });
              process.send({ready:true});
            } catch(error) { process.send({error:String(error)}); }
          });
			process.on('SIGTERM', async()=>{socket?.close();await server.close();process.exit(0)});
		`,
			],
			{ stdio: ["ignore", "pipe", "pipe", "ipc"] },
		);
		const port = await new Promise<string>((resolve, reject) => {
			server.stdout!.on("data", (chunk) => {
				const match = String(chunk).match(/FIXTURE_PORT=(\d+)/);
				if (match) resolve(match[1]);
			});
			server.once("error", reject);
			server.once("exit", (code) => reject(new Error(`Fixture exited ${code}`)));
		});
		const url = `http://127.0.0.1:${port}/src/__tests__/browser/aiChatComposerGeometry.html`;
		try {
			for (const phone of [false, true]) {
				run("mac-chrome", `${url}${phone ? "?mobile" : ""}`);
				run("--", "set", "viewport", phone ? "390" : "1400", "900");
				run("--", "wait", "--fn", "window.controlGeometryReady === true");
				const endpoint = run("--", "get", "cdp-url").trim().replace(/^"|"$/g, "");
				cdpPort = new URL(endpoint).port;
				// The child has native browser APIs; app test setup deliberately stubs these in Vitest.
				await new Promise<void>((resolve, reject) => {
					server.once("message", (message: { ready?: boolean; error?: string }) =>
						message.error ? reject(new Error(message.error)) : resolve(),
					);
					server.send({ cdpPort, url });
				});
				for (const [name, text] of [
					["empty", ""],
					["one", "il mio goal è di"],
					["multi", "First line\nSecond line\nThird line\nFourth line\nFifth line"],
				]) {
					for (const busy of phone ? [false] : [false, true]) {
						for (const focus of [false, true]) {
							const sample = JSON.parse(
								run(
									"--",
									"eval",
									`window.composerSample(${JSON.stringify(phone && name === "multi" ? text.split("\n").slice(0, 4).join("\n") : text)}, ${focus}, ${busy})`,
								),
							) as Sample;
							(phone ? mobile : samples).push(sample);
							run(
								"--",
								"screenshot",
								join(
									directory!,
									`${phase}-${phone ? "mobile" : "desktop"}-${name}-${busy ? "busy" : "idle"}-${focus ? "focus" : "blur"}.png`,
								),
							);
						}
					}
				}
			}
			writeFileSync(join(directory!, `${phase}.json`), JSON.stringify({ samples, mobile }, null, 2));
		} finally {
			run("--", "tab", "close");
		}
	}, 240000);
	afterAll(async () => {
		if (daemon) execFileSync("agent-browser", ["--session", daemon, "close"], { timeout: 45000, stdio: "pipe" });
		if (server?.pid && server.exitCode === null) {
			const closed = new Promise<void>((resolve) => server.once("close", () => resolve()));
			server.kill("SIGTERM");
			await closed;
		}
	});
	it("keeps Send and Stop fixed-size and bottom-aligned — catches actions stretching with multiline drafts", () => {
		expect(samples).toHaveLength(12);
		for (const sample of samples) {
			for (const action of [sample.send, ...(sample.stop ? [sample.stop] : [])]) {
				expect.soft(action.height, JSON.stringify(sample)).toBeCloseTo(36, 1);
				expect.soft(action.bottom, JSON.stringify(sample)).toBeCloseTo(sample.editor.bottom, 1);
			}
			expect.soft(sample.send.width, JSON.stringify(sample)).toBeCloseTo(36, 1);
			expect.soft(!!sample.stop).toBe(sample.busy);
			expect.soft(sample.editor.right).toBeLessThan((sample.stop ?? sample.send).left);
		}
	});
	it("retains shared content edges and focus band — catches misplaced rows or an offset editor focus ring", () => {
		for (const sample of samples) {
			expect.soft(sample.editor.left).toBeCloseTo(sample.header.left, 1);
			expect.soft(sample.send.right).toBeCloseTo(sample.header.right, 1);
			expect.soft(sample.footer.left).toBeCloseTo(sample.header.left, 1);
			expect.soft(sample.footer.right).toBeCloseTo(sample.header.right, 1);
			// The focus band must hug the border, not extend into a different content inset.
			expect.soft(sample.actualFocus).toBe(sample.focus);
			if (sample.focus) {
				expect.soft(sample.focusVisible).toBe(true);
				expect.soft(sample.focusStyle.outline).toBe("none");
				expect.soft(sample.focusStyle.shadow).toContain("inset");
			}
		}
	});
	it("preserves mobile composer bounds — catches desktop alignment rules leaking into the phone editor", () => {
		expect(mobile).toHaveLength(6);
		if (phase === "before") return;
		const baseline = JSON.parse(
			readFileSync(new URL("./aiChatComposerMobileBaseline.json", import.meta.url), "utf8"),
		) as Sample[];
		for (const [index, sample] of mobile.entries()) {
			expect(sample.editor).toEqual(baseline[index].editor);
			expect(sample.send).toEqual(baseline[index].send);
		}
	});
});
