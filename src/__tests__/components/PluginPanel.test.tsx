import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginPanelTab } from "../../stores/mdTabs";

// Mock pluginRegistry to avoid Tauri calls
vi.mock("../../plugins/pluginRegistry", () => ({
	pluginRegistry: {
		handlePanelMessage: vi.fn(),
		registerPanelSendChannel: vi.fn(),
		unregisterPanelSendChannel: vi.fn(),
		setPanelVisible: vi.fn(),
	},
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
	})),
}));

vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn() }));

// Track whether addEventListener("message") was called inside an onMount callback.
// We wrap solid-js onMount to set a flag during its execution.
let insideOnMount = false;
let messageListenerCalledInsideOnMount: boolean | null = null;

vi.mock("solid-js", async (importOriginal) => {
	const actual = await importOriginal<typeof import("solid-js")>();
	return {
		...actual,
		onMount: (fn: () => void) => {
			return actual.onMount(() => {
				insideOnMount = true;
				fn();
				insideOnMount = false;
			});
		},
	};
});

import { invoke } from "@tauri-apps/api/core";
import { createSignal, For } from "solid-js";
import { injectThemeVars, PluginPanel } from "../../components/PluginPanel/PluginPanel";
import { TUIC_SDK_SCRIPT } from "../../components/PluginPanel/tuicSdk";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { applyAppTheme } from "../../themes";
import { handleOpenUrl } from "../../utils/openUrl";

function makeTab(overrides: Partial<PluginPanelTab> = {}): PluginPanelTab {
	return {
		id: "tab-1",
		type: "plugin-panel",
		pluginId: "test-plugin",
		title: "Test Plugin",
		html: "<html><body>hello</body></html>",
		...overrides,
	} as PluginPanelTab;
}

describe("PluginPanel", () => {
	let addEventListenerSpy: ReturnType<typeof vi.spyOn>;
	let removeEventListenerSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		vi.useFakeTimers();
		insideOnMount = false;
		messageListenerCalledInsideOnMount = null;

		const originalAdd = window.addEventListener.bind(window);
		addEventListenerSpy = vi
			.spyOn(window, "addEventListener")
			.mockImplementation((event: string, ...rest: unknown[]) => {
				if (event === "message") {
					messageListenerCalledInsideOnMount = insideOnMount;
				}
				return originalAdd(
					event as keyof WindowEventMap,
					...(rest as [EventListenerOrEventListenerObject, (boolean | AddEventListenerOptions)?]),
				);
			});
		removeEventListenerSpy = vi.spyOn(window, "removeEventListener");
	});

	afterEach(async () => {
		vi.runAllTicks();
		vi.advanceTimersByTime(0);
		await vi.runAllTimersAsync();
		vi.useRealTimers();
		addEventListenerSpy.mockRestore();
		removeEventListenerSpy.mockRestore();
	});

	it("addEventListener('message') is called inside onMount — not at component body evaluation", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		// The message listener MUST have been registered inside onMount, not at component body level
		expect(messageListenerCalledInsideOnMount).toBe(true);
	});

	it("registers message listener on window during mount", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		const messageCalls = addEventListenerSpy.mock.calls.filter(([event]: [string]) => event === "message");
		expect(messageCalls).toHaveLength(1);
	});

	it("opens an external URL only when the owning plugin iframe sends the message", () => {
		const { container } = render(() => <PluginPanel tab={makeTab()} />);
		const iframe = container.querySelector("iframe") as HTMLIFrameElement;
		const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
		const foreign = new MessageEvent("message", { data: { type: "tuic:open-url", url: "https://example.org/help" } });
		(handler as EventListener)(foreign);
		expect(handleOpenUrl).not.toHaveBeenCalled();
		const own = new MessageEvent("message", { data: { type: "tuic:open-url", url: "https://example.org/help" } });
		Object.defineProperty(own, "source", { get: () => iframe.contentWindow });
		(handler as EventListener)(own);
		expect(handleOpenUrl).toHaveBeenCalledWith("https://example.org/help");
	});

	it("does not let a URL-mode dashboard request browser opens through the message bridge", () => {
		const { container } = render(() => <PluginPanel tab={makeTab({ url: "about:blank", html: "" })} />);
		const iframe = container.querySelector("iframe") as HTMLIFrameElement;
		const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
		const request = new MessageEvent("message", {
			data: { type: "tuic:open-url", url: "https://example.org/help" },
		});
		Object.defineProperty(request, "source", { get: () => iframe.contentWindow });
		(handler as EventListener)(request);
		expect(handleOpenUrl).not.toHaveBeenCalled();
	});

	describe("tuic:open of an image (#2715-62aa)", () => {
		const REPO = "/Users/boss/Gits/fabbrica";

		function sendOpen(path: string) {
			const { container } = render(() => <PluginPanel tab={makeTab()} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
			const msg = new MessageEvent("message", { data: { type: "tuic:open", path } });
			Object.defineProperty(msg, "source", { get: () => iframe.contentWindow });
			(handler as EventListener)(msg);
		}

		beforeEach(() => {
			mdTabsStore.clearAll();
			repositoriesStore.add({ path: REPO, displayName: "fabbrica" });
		});

		afterEach(() => {
			mdTabsStore.clearAll();
			repositoriesStore.remove(REPO);
			repositoriesStore._testCancelPendingSave();
		});

		// Catches: binary file sent to the UTF-8 text reader ("Failed to read file:
		// stream did not contain valid UTF-8").
		it.each(["png", "jpg", "gif", "webp", "svg"])("open_png_path_renders_image_view (.%s)", (ext) => {
			sendOpen(`${REPO}/evidence/header-dark.${ext}`);
			const tabs = Object.values(mdTabsStore.state.tabs);
			expect(tabs).toHaveLength(1);
			expect(tabs[0]).toMatchObject({
				type: "html-preview",
				repoPath: REPO,
				filePath: `evidence/header-dark.${ext}`,
			});
		});

		// Catches: the image branch swallowing documents that the markdown reader owns.
		it("keeps a markdown file in the markdown reader", () => {
			sendOpen(`${REPO}/docs/readme.md`);
			expect(Object.values(mdTabsStore.state.tabs)).toEqual([
				expect.objectContaining({ type: "file", filePath: "docs/readme.md" }),
			]);
		});
	});

	describe("tuic:open and tuic:edit of an absolute path outside every repo (#2666-17b6)", () => {
		const PANEL_REPO = "/Users/boss/Gits/panel-repo";
		const FOCUSED = "/Users/boss/Gits/focused-repo";
		const OUTSIDE = "/Users/boss/Reference";

		function send(data: Record<string, unknown>) {
			const { container } = render(() => <PluginPanel tab={makeTab({ repoPath: PANEL_REPO })} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;
			const msg = new MessageEvent("message", { data });
			Object.defineProperty(msg, "source", { get: () => iframe.contentWindow });
			(handler as EventListener)(msg);
		}

		beforeEach(() => {
			mdTabsStore.clearAll();
			editorTabsStore.clearAll();
			repositoriesStore.add({ path: PANEL_REPO, displayName: "panel" });
			repositoriesStore.add({ path: FOCUSED, displayName: "focused" });
			repositoriesStore.setActive(FOCUSED);
		});

		afterEach(() => {
			mdTabsStore.clearAll();
			editorTabsStore.clearAll();
			repositoriesStore.remove(PANEL_REPO);
			repositoriesStore.remove(FOCUSED);
			repositoriesStore._testCancelPendingSave();
		});

		// Catches: owner report cannot open an accessible external reference file.
		it("plugin_panel_absolute_external_file_opens_and_edits: open", () => {
			send({ type: "tuic:open", path: `${OUTSIDE}/notes.md` });
			expect(Object.values(mdTabsStore.state.tabs)).toEqual([
				expect.objectContaining({ type: "file", repoPath: PANEL_REPO, filePath: `${OUTSIDE}/notes.md` }),
			]);
		});

		it("plugin_panel_absolute_external_file_opens_and_edits: open of an image goes to the preview tab", () => {
			send({ type: "tuic:open", path: `${OUTSIDE}/shot.png` });
			expect(Object.values(mdTabsStore.state.tabs)).toEqual([
				expect.objectContaining({ type: "html-preview", repoPath: PANEL_REPO, filePath: `${OUTSIDE}/shot.png` }),
			]);
		});

		it("plugin_panel_absolute_external_file_opens_and_edits: edit opens an editable external editor tab", () => {
			send({ type: "tuic:edit", path: `${OUTSIDE}/notes.txt`, line: 7 });
			expect(Object.values(editorTabsStore.state.tabs)).toEqual([
				expect.objectContaining({
					repoPath: PANEL_REPO,
					filePath: `${OUTSIDE}/notes.txt`,
					initialLine: 7,
					externalEditable: true,
				}),
			]);
		});

		// Catches: panel-relative link resolved into the focused foreign repo.
		it("panel_relative_link_cannot_follow_foreign_focused_repo: relative open stays in the panel repo", () => {
			send({ type: "tuic:open", path: "docs/a.md" });
			expect(Object.values(mdTabsStore.state.tabs)).toEqual([
				expect.objectContaining({ repoPath: PANEL_REPO, filePath: "docs/a.md" }),
			]);
		});

		it("a relative path that escapes the panel repo still opens nothing", () => {
			send({ type: "tuic:open", path: "../../secret.md" });
			expect(Object.values(mdTabsStore.state.tabs)).toHaveLength(0);
		});

		// Catches: tuic.getFile gaining external reads through the shared resolver (2644/2650 own that).
		it("get-file does not read an external path through fs_read_file", () => {
			vi.mocked(invoke).mockClear();
			send({ type: "tuic:get-file", path: `${OUTSIDE}/notes.md`, requestId: 1 });
			expect(invoke).not.toHaveBeenCalled();
		});
	});

	it("removes message listener on unmount", () => {
		const tab = makeTab();
		const { unmount } = render(() => <PluginPanel tab={tab} />);

		// Capture which handler was registered
		const [, registeredHandler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;

		unmount();

		const removeCalls = removeEventListenerSpy.mock.calls.filter(([event]: [string]) => event === "message");
		expect(removeCalls).toHaveLength(1);
		expect(removeCalls[0][1]).toBe(registeredHandler);
	});

	it("registers send channel via pluginRegistry on mount", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		expect(pluginRegistry.registerPanelSendChannel).toHaveBeenCalledWith("tab-1", expect.any(Function));
	});

	it.each([{ answer: 42 }, "hello", null])("host send reaches tuic.onMessage for %j", (payload) => {
		const { container } = render(() => <PluginPanel tab={makeTab()} />);
		const frame = container.querySelector("iframe")!.contentWindow!;
		// Execute the shipped SDK in its real receiving window, then deliver the
		// mounted host channel's postMessage output across that window boundary.
		(frame as Window & typeof globalThis).eval(
			TUIC_SDK_SCRIPT.replace(/^<script[^>]*>/, "").replace(/<\/script>$/, ""),
		);
		const received: unknown[] = [];
		(frame as unknown as { tuic: { onMessage: (cb: (data: unknown) => void) => void } }).tuic.onMessage((data) =>
			received.push(data),
		);
		vi.spyOn(frame, "postMessage").mockImplementation((data) => {
			frame.dispatchEvent(new MessageEvent("message", { data, source: window }));
		});
		const send = vi.mocked(pluginRegistry.registerPanelSendChannel).mock.calls.at(-1)![1];
		send(payload);
		expect(received).toEqual([payload]);
	});

	it("forwards transferable ownership through iframe postMessage", () => {
		const tab = makeTab();
		const { container } = render(() => <PluginPanel tab={tab} />);
		const iframe = container.querySelector("iframe");
		expect(iframe?.contentWindow).toBeTruthy();
		let received: { type: string; payload: { type: string; buffer: ArrayBuffer } } | undefined;
		// happy-dom does not transfer ownership; use the native clone operation
		// at the iframe boundary so dropping the transfer list fails this test.
		vi.spyOn(iframe!.contentWindow!, "postMessage").mockImplementation(
			(data: unknown, target?: string | WindowPostMessageOptions, transfer?: Transferable[]) => {
				received = structuredClone(data as NonNullable<typeof received>, {
					transfer: typeof target === "object" ? target.transfer : transfer,
				});
			},
		);
		const send = vi.mocked(pluginRegistry.registerPanelSendChannel).mock.calls.at(-1)?.[1];
		const buffer = new ArrayBuffer(4);
		new Uint8Array(buffer).set([1, 2, 3, 4]);

		send?.({ type: "database", buffer }, [buffer]);

		expect(buffer.byteLength).toBe(0);
		expect(received?.type).toBe("tuic:host-message");
		expect(received?.payload.type).toBe("database");
		expect(Array.from(new Uint8Array(received!.payload.buffer))).toEqual([1, 2, 3, 4]);
	});

	it("unregisters send channel via pluginRegistry on unmount", () => {
		const tab = makeTab();
		const { unmount } = render(() => <PluginPanel tab={tab} />);
		unmount();

		expect(pluginRegistry.unregisterPanelSendChannel).toHaveBeenCalledWith("tab-1");
	});

	it("routes non-system messages without throwing (source guard prevents routing when no iframe)", () => {
		const tab = makeTab();
		render(() => <PluginPanel tab={tab} />);

		// Get the registered handler
		const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;

		// Simulate a message from an unknown source — handler guards on iframeRef.contentWindow
		const fakeEvent = new MessageEvent("message", {
			data: { type: "custom", payload: "test" },
		});
		expect(() => (handler as EventListener)(fakeEvent)).not.toThrow();
	});

	it("renders an iframe element", () => {
		const tab = makeTab();
		const { container } = render(() => <PluginPanel tab={tab} />);
		const iframe = container.querySelector("iframe");
		expect(iframe).not.toBeNull();
	});

	it("renders iframe with sandbox allow-scripts allow-same-origin", () => {
		const tab = makeTab();
		const { container } = render(() => <PluginPanel tab={tab} />);
		const iframe = container.querySelector("iframe");
		expect(iframe?.getAttribute("sandbox")).toBe("allow-scripts allow-same-origin");
	});

	describe("srcdoc writes (613-00e8 F100)", () => {
		/**
		 * Count assignments to the iframe's srcdoc. Reassigning it navigates the
		 * iframe: a fresh document, a fresh JS global, and the scroll position,
		 * focus and in-page state of the old one gone. It must happen only when the
		 * HTML actually changed.
		 */
		function countSrcdocWrites(iframe: HTMLIFrameElement) {
			const counter = { writes: 0 };
			let value = iframe.getAttribute("srcdoc") ?? "";
			Object.defineProperty(iframe, "srcdoc", {
				configurable: true,
				get: () => value,
				set: (next: string) => {
					counter.writes++;
					value = next;
				},
			});
			const realSetAttribute = iframe.setAttribute.bind(iframe);
			iframe.setAttribute = (name: string, next: string) => {
				if (name === "srcdoc") {
					counter.writes++;
					value = next;
					return;
				}
				realSetAttribute(name, next);
			};
			return counter;
		}

		it("re-renders the iframe only when the plugin's HTML actually changed", () => {
			mdTabsStore.clearAll();
			const tabId = mdTabsStore.addPluginPanel("p1", "dash", "Dash", "<html><body>v1</body></html>");
			const { container } = render(() => <PluginPanel tab={mdTabsStore.get(tabId) as PluginPanelTab} />);
			const counter = countSrcdocWrites(container.querySelector("iframe") as HTMLIFrameElement);

			mdTabsStore.updatePluginPanel(tabId, "<html><body>v1</body></html>");
			expect(counter.writes).toBe(0);

			mdTabsStore.updatePluginPanel(tabId, "<html><body>v2</body></html>");
			expect(counter.writes).toBe(1);
		});
	});

	describe("hidden panels (613-00e8 F102)", () => {
		it.each([
			["URL", { url: "about:blank", html: "" }],
			["pinned URL", { url: "about:blank", html: "", pinned: true }],
			["inline plugin", { html: "<p>Dashboard</p>" }],
		])("unloads a hidden %s iframe and reloads it when shown", (_mode, overrides) => {
			const [visible, setVisible] = createSignal(true);
			const { container } = render(() => <PluginPanel tab={makeTab(overrides)} visible={visible} />);
			const first = container.querySelector("iframe") as HTMLIFrameElement;
			const src = first.getAttribute("src");
			const srcdoc = first.getAttribute("srcdoc");
			setVisible(false);
			expect(container.querySelector("iframe")).toBeNull();
			setVisible(true);
			const second = container.querySelector("iframe") as HTMLIFrameElement;
			expect(second).not.toBe(first);
			expect(second.getAttribute("src")).toBe(src);
			expect(second.getAttribute("srcdoc")).toBe(srcdoc);
		});

		it("removes the iframe when the UI tab closes", () => {
			mdTabsStore.clearAll();
			mdTabsStore.openUiTab("closing-panel", "Dashboard", "", false, "about:blank");
			const { container } = render(() => (
				<For each={mdTabsStore.getIds()}>
					{(id) => {
						const tab = mdTabsStore.get(id) as PluginPanelTab;
						return <PluginPanel tab={tab} />;
					}}
				</For>
			));
			expect(container.querySelectorAll("iframe")).toHaveLength(1);

			mdTabsStore.closeUiTab("closing-panel");
			expect(container.querySelectorAll("iframe")).toHaveLength(0);
		});

		it("unloads only the hidden iframe when two panels share the page", () => {
			const [firstVisible, setFirstVisible] = createSignal(true);
			const [secondVisible, setSecondVisible] = createSignal(true);
			const { container } = render(() => (
				<>
					<PluginPanel tab={makeTab({ id: "first", url: "about:blank#first", html: "" })} visible={firstVisible} />
					<PluginPanel tab={makeTab({ id: "second", url: "about:blank#second", html: "" })} visible={secondVisible} />
				</>
			));
			expect(container.querySelectorAll("iframe")).toHaveLength(2);

			setFirstVisible(false);
			expect([...container.querySelectorAll("iframe")].map((frame) => frame.getAttribute("src"))).toEqual([
				"about:blank#second",
			]);

			setSecondVisible(false);
			expect(container.querySelectorAll("iframe")).toHaveLength(0);
		});
		/**
		 * Mount a panel and start spying on what reaches its iframe. The stub goes
		 * in after mount on purpose — the mount-time handshake is not what this
		 * block is about.
		 */
		let contentWindowSpy: ReturnType<typeof vi.spyOn> | undefined;
		function spyOnPanelTraffic(visible: () => boolean) {
			const tab = makeTab();
			const postMessage = vi.fn();
			contentWindowSpy = vi
				.spyOn(HTMLIFrameElement.prototype, "contentWindow", "get")
				.mockReturnValue({ postMessage } as unknown as Window);
			render(() => <PluginPanel tab={tab} visible={visible} />);
			postMessage.mockClear();
			return postMessage;
		}

		const repoMessages = (postMessage: ReturnType<typeof vi.fn>) =>
			postMessage.mock.calls.filter((call) => call[0]?.type === "tuic:repo-changed");

		afterEach(() => {
			contentWindowSpy?.mockRestore();
			contentWindowSpy = undefined;
			repositoriesStore.setActive(null);
			repositoriesStore._testCancelPendingSave();
		});

		it("does not push repo changes at a panel nobody can see", () => {
			const postMessage = spyOnPanelTraffic(() => false);

			repositoriesStore.setActive("/repo-x");

			expect(repoMessages(postMessage)).toHaveLength(0);
		});

		it("hands the current repo over the moment the panel is shown", () => {
			const [visible, setVisible] = createSignal(false);
			const postMessage = spyOnPanelTraffic(visible);

			repositoriesStore.setActive("/repo-x");
			expect(repoMessages(postMessage)).toHaveLength(0);

			setVisible(true);

			// Exactly one — the panel must learn the current repo, not replay every
			// repo it missed while hidden.
			expect(repoMessages(postMessage)).toHaveLength(1);
			expect(repoMessages(postMessage)[0][0]).toEqual({ type: "tuic:repo-changed", repoPath: "/repo-x" });
		});

		it("keeps pushing at a visible panel", () => {
			const postMessage = spyOnPanelTraffic(() => true);

			repositoriesStore.setActive("/repo-x");

			expect(repoMessages(postMessage)).toHaveLength(1);
		});
	});

	describe("theme extraction (613-00e8 F103)", () => {
		const HTML = "<html><head></head><body></body></html>";
		let sheetReads = 0;

		beforeEach(() => {
			// Invalidate whatever earlier tests left in the cache, so the first
			// injection below is guaranteed to be the one that walks the sheets.
			applyAppTheme("vscode-dark");
			sheetReads = 0;
			// StyleSheetList is live, so holding the object is enough to hand back.
			const real = document.styleSheets;
			Object.defineProperty(document, "styleSheets", {
				configurable: true,
				get() {
					sheetReads++;
					return real;
				},
			});
		});

		afterEach(() => {
			delete (document as unknown as Record<string, unknown>).styleSheets;
		});

		it("walks the stylesheets once and reuses the result for every later injection", () => {
			injectThemeVars(HTML, false);
			const afterFirst = sheetReads;
			expect(afterFirst).toBeGreaterThan(0);

			injectThemeVars(HTML, false);
			injectThemeVars(HTML, true);

			expect(sheetReads).toBe(afterFirst);
		});

		it("re-reads the stylesheets once applyAppTheme has rewritten the root variables", () => {
			injectThemeVars(HTML, false);
			const afterFirst = sheetReads;

			applyAppTheme("vscode-dark");
			injectThemeVars(HTML, false);

			expect(sheetReads).toBeGreaterThan(afterFirst);
		});
	});

	describe("URL mode — SDK handshake", () => {
		function makeUrlTab(): PluginPanelTab {
			return {
				id: "tab-url",
				type: "plugin-panel",
				pluginId: "test-plugin",
				title: "URL Plugin",
				html: "",
				url: "about:blank",
			} as PluginPanelTab;
		}

		it("renders URL iframe with allow-scripts allow-same-origin sandbox", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe");
			expect(iframe?.getAttribute("sandbox")).toBe("allow-scripts allow-same-origin");
			expect(iframe?.getAttribute("src")).toBe("about:blank");
		});

		it("posts tuic:sdk-init to the iframe on load", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			// Stub contentWindow.postMessage — jsdom gives us a real window, we spy on it
			const postMessageSpy = vi.fn();
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => ({ postMessage: postMessageSpy }),
			});

			// Trigger the onLoad handler
			iframe.dispatchEvent(new Event("load"));

			// sendSdkInit posts three messages: sdk-init + repo-changed + theme-changed
			expect(postMessageSpy).toHaveBeenCalledTimes(3);
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:sdk-init", version: "1.0" }, "*");
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:repo-changed", repoPath: null }, "*");
		});

		it("responds to tuic:sdk-request with tuic:sdk-init (async-listener fallback)", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			const postMessageSpy = vi.fn();
			const fakeContentWindow = { postMessage: postMessageSpy };
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => fakeContentWindow,
			});

			// Get the registered window message handler
			const [, handler] = addEventListenerSpy.mock.calls.find(([event]: [string]) => event === "message")!;

			// Simulate the child sending tuic:sdk-request after its listener became ready
			const event = new MessageEvent("message", {
				data: { type: "tuic:sdk-request" },
			});
			// Force event.source to match contentWindow so the source guard passes
			Object.defineProperty(event, "source", {
				get: () => fakeContentWindow,
			});

			(handler as EventListener)(event);

			// sendSdkInit posts three messages: sdk-init + repo-changed + theme-changed
			expect(postMessageSpy).toHaveBeenCalledTimes(3);
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:sdk-init", version: "1.0" }, "*");
			expect(postMessageSpy).toHaveBeenCalledWith({ type: "tuic:repo-changed", repoPath: null }, "*");
		});

		it("sdk-init is idempotent across repeated onLoad (e.g., in-iframe navigation)", () => {
			const tab = makeUrlTab();
			const { container } = render(() => <PluginPanel tab={tab} />);
			const iframe = container.querySelector("iframe") as HTMLIFrameElement;

			const postMessageSpy = vi.fn();
			Object.defineProperty(iframe, "contentWindow", {
				configurable: true,
				get: () => ({ postMessage: postMessageSpy }),
			});

			iframe.dispatchEvent(new Event("load"));
			iframe.dispatchEvent(new Event("load"));
			iframe.dispatchEvent(new Event("load"));

			// 3 loads × 3 messages each (sdk-init + repo-changed + theme-changed) = 9
			expect(postMessageSpy).toHaveBeenCalledTimes(9);
			const sdkInitCalls = postMessageSpy.mock.calls.filter((call) => call[0]?.type === "tuic:sdk-init");
			expect(sdkInitCalls).toHaveLength(3);
			for (const call of sdkInitCalls) {
				expect(call[0]).toEqual({ type: "tuic:sdk-init", version: "1.0" });
			}
		});
	});
});
