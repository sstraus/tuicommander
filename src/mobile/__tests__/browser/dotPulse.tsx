import { render } from "solid-js/web";
import { OutputView } from "../../components/OutputView";
import fixture from "../fixtures/claude-dot-blink-mobile-1654.json";
import "../../mobile.css";

// The transport seam feeds recorded producer text to the real OutputView.
// Run only with src/mobile/__tests__/browser/dotPulse.vite.config.ts; no live sessions or backend writes.
window.fetch = async () => new Response(JSON.stringify({ lines: [], screen: [], total_lines: 0 }));
document.body.style.cssText = "margin:0; background:#151515; color:#ddd";
document.getElementById("root")!.style.cssText = "height:100vh;display:flex;flex-direction:column";
render(() => <OutputView sessionId="dot-fixture" />, document.getElementById("root")!);
Object.assign(window, {
	dotSource: fixture.toolLine,
	setDotFrame(state: "on" | "off" | "green", text = fixture.toolLine) {
		const row = state === "off" ? text.replace(/^⏺/, " ") : text;
		(window as unknown as { emitDotRows: (rows: unknown[]) => void }).emitDotRows([
			{ spans: [{ text: row, fg: { rgb: state === "green" ? [78, 186, 101] : [153, 153, 153] } }], cols: 160 },
			{ spans: [{ text: "" }], cols: 160 },
			{ spans: [{ text: "AFTER_BLOCK" }], cols: 160 },
		]);
	},
});
