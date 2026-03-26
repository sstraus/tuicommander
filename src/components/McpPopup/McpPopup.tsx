import { Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { mcpPopupStore } from "../../stores/mcpPopup";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { invoke } from "../../invoke";
import s from "./McpPopup.module.css";

/** Mirrors the status snapshot returned by get_mcp_upstream_status */
interface UpstreamStatusEntry {
  name: string;
  status: "connecting" | "ready" | "circuit_open" | "disabled" | "failed";
  transport: { type: string };
  tool_count: number;
}

const STATUS_COLORS: Record<string, string> = {
  ready: "#98c379",
  connecting: "#e5c07b",
  circuit_open: "#e06c75",
  disabled: "#5c6370",
  failed: "#e06c75",
};

export const McpPopup: Component<{ onOpenSettings: (tab: string) => void }> = (props) => {
  const [servers, setServers] = createSignal<UpstreamStatusEntry[]>([]);
  let overlayRef: HTMLDivElement | undefined;

  // Poll upstream status while open
  createEffect(() => {
    if (!mcpPopupStore.state.isOpen) return;

    const refresh = () =>
      invoke<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status")
        .then((snap) => setServers(snap?.upstreams ?? []))
        .catch(() => {});

    refresh();
    const timer = setInterval(refresh, 3000);
    onCleanup(() => clearInterval(timer));
  });

  // ESC to close
  createEffect(() => {
    if (!mcpPopupStore.state.isOpen) return;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        mcpPopupStore.close();
      }
    };
    document.addEventListener("keydown", onKey);
    onCleanup(() => document.removeEventListener("keydown", onKey));
  });

  const repoPath = () => repositoriesStore.state.activeRepoPath;
  const repoName = () => {
    const p = repoPath();
    return p ? p.split("/").pop() ?? p : "No repo";
  };

  /** Current allowlist — null means all servers */
  const allowlist = (): string[] | null => {
    const p = repoPath();
    if (!p) return null;
    const effective = repoSettingsStore.getEffective(p);
    return effective?.mcpUpstreams ?? null;
  };

  /** Whether a server is enabled for this repo */
  const isEnabled = (name: string): boolean => {
    const list = allowlist();
    return list === null || list.includes(name);
  };

  /** Toggle a server in the per-repo allowlist */
  const toggleServer = (name: string) => {
    const p = repoPath();
    if (!p) return;

    const current = allowlist();
    const allNames = servers().map((s) => s.name);

    let next: string[] | null;
    if (current === null) {
      // Currently "all" — switching to explicit list minus this server
      next = allNames.filter((n) => n !== name);
    } else if (current.includes(name)) {
      // Remove from allowlist
      const filtered = current.filter((n) => n !== name);
      // If all remaining servers are selected, go back to null (= all)
      next = filtered.length === allNames.length ? null : filtered;
    } else {
      // Add to allowlist
      const added = [...current, name];
      next = added.length >= allNames.length ? null : added;
    }

    repoSettingsStore.update(p, { mcpUpstreams: next });
  };

  const handleOverlayClick = (e: MouseEvent) => {
    if (e.target === overlayRef) mcpPopupStore.close();
  };

  const openServicesTab = () => {
    mcpPopupStore.close();
    props.onOpenSettings("services");
  };

  return (
    <Show when={mcpPopupStore.state.isOpen}>
      <div class={s.overlay} ref={overlayRef} onClick={handleOverlayClick}>
        <div class={s.popup}>
          <div class={s.header}>
            <span class={s.title}>MCP Servers ({repoName()})</span>
          </div>

          <div class={s.list}>
            <Show when={servers().length === 0}>
              <div class={s.empty}>No upstream MCP servers configured</div>
            </Show>
            <For each={servers()}>
              {(server) => (
                <div class={s.item} onClick={() => toggleServer(server.name)}>
                  <span
                    class={s.statusDot}
                    style={{ background: STATUS_COLORS[server.status] ?? "#5c6370" }}
                    title={server.status.replace("_", " ")}
                  />
                  <div class={s.info}>
                    <span class={s.name}>{server.name}</span>
                    <span
                      class={`${s.badge} ${server.transport.type === "http" ? s.badgeHttp : s.badgeStdio}`}
                    >
                      {server.transport.type.toUpperCase()}
                    </span>
                  </div>
                  <span class={s.tools}>{server.tool_count} tools</span>
                  <input
                    type="checkbox"
                    class={s.checkbox}
                    checked={isEnabled(server.name)}
                    onChange={() => toggleServer(server.name)}
                    onClick={(e) => e.stopPropagation()}
                  />
                </div>
              )}
            </For>
          </div>

          <div class={s.footer}>
            <button class={s.footerLink} onClick={openServicesTab}>
              Manage in Settings
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};
