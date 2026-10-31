import { render } from "solid-js/web";
import { ContextualTip } from "../../components/TipOfTheDay/ContextualTip";
import "../../global.css";
import "../../styles.css";

// Visual fixture: the real inactive panel, isolated internal transport responses.
// No daily backend, user config, hook installation or ego process is contacted.
window.fetch = async () => { throw new Error("Network is disabled in the onboarding fixture"); };
const callbacks = new Map();
window.__TAURI_INTERNALS__ = {
  transformCallback: callback => { const id = callbacks.size + 1; callbacks.set(id, callback); return id; },
  unregisterCallback: id => callbacks.delete(id),
  invoke: async command => {
    if (command === "detect_all_agent_binaries") return new URLSearchParams(location.search).has("missing") ? {} : { ego: { path: "/Users/example/.local/bin/ego", version: null } };
    if (command === "load_config") return { ego_executable: "", ai_chat_sessions: {} };
    if (command === "ai_chat_selection") return { sessionId: null };
    if (command === "plugin:event|listen") return 1;
    if (command === "acp_workspace_root") return "/fixture/repository";
    return undefined;
  },
};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
localStorage.removeItem("tuic.dismissed-tip.hooks");
localStorage.removeItem("tuic.dismissed-tip.autoRetry");
localStorage.removeItem("tuic.dismissed-tip.preventSleep");
const { AIChatPanel } = await import("../../components/AIChatPanel/AIChatPanel");
render(() => <main style={{ display: "flex", gap: "16px", padding: "16px", height: "850px" }}>
  <section style={{ width: "500px", height: "700px", display: "flex" }}>
    <AIChatPanel visible={true} repoPath={null} onClose={() => {}} />
  </section>
  <section style={{ display: "flex", "flex-direction": "column", gap: "16px", width: "600px" }}>
    <ContextualTip id="preventSleep" when={true} />
    <ContextualTip id="hooks" when={true} />
    <ContextualTip id="autoRetry" when={true} />
  </section>
</main>, document.getElementById("root"));
window.controlGeometryReady = true;
