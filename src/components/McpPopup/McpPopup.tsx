import { Component, createEffect, For, onCleanup, Show } from "solid-js";
import { mcpPopupStore } from "../../stores/mcpPopup";
import type { UpstreamStatusEntry } from "../../stores/mcpPopup";
import s from "./McpPopup.module.css";

const STATUS_COLORS: Record<string, string> = {
  ready: "#98c379",
  connecting: "#e5c07b",
  circuit_open: "#e06c75",
  disabled: "#5c6370",
  failed: "#e06c75",
  authenticating: "#61afef",
  needs_auth: "#e5c07b",
};

export const McpPopup: Component<{ onOpenSettings: (tab: string) => void }> = (props) => {
  let overlayRef: HTMLDivElement | undefined;

  // Event-driven status refresh + fallback poll while open
  createEffect(() => {
    if (!mcpPopupStore.state.isOpen) return;

    // Listen for upstream-status-changed events
    let unlistenFn: (() => void) | undefined;
    mcpPopupStore.listenForStatusChanges().then((fn) => {
      unlistenFn = fn;
    });

    // Slow fallback poll (10s) for robustness
    const timer = setInterval(() => mcpPopupStore.refreshStatus(), 10_000);

    onCleanup(() => {
      clearInterval(timer);
      unlistenFn?.();
    });
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

  /** Get live status entry for a server by name */
  const getStatus = (name: string): UpstreamStatusEntry | undefined =>
    mcpPopupStore.state.status.find((u) => u.name === name);

  /** Whether a server is enabled (from config, not status) */
  const isEnabled = (name: string): boolean => {
    const server = mcpPopupStore.state.servers.find((s) => s.name === name);
    return server?.enabled ?? true;
  };

  const handleOverlayClick = (e: MouseEvent) => {
    if (e.target === overlayRef) mcpPopupStore.close();
  };

  const openServicesTab = () => {
    mcpPopupStore.close();
    props.onOpenSettings("services");
  };

  // Merge config servers with status for display — config is source of truth for the list
  const displayServers = () => {
    const servers = mcpPopupStore.state.servers;
    if (servers.length > 0) return servers;
    // Fallback: if config hasn't loaded yet, show status-only entries
    return mcpPopupStore.state.status.map((st) => ({
      name: st.name,
      enabled: st.status !== "disabled",
      transport: st.transport,
    }));
  };

  return (
    <Show when={mcpPopupStore.state.isOpen}>
      <div class={s.overlay} ref={overlayRef} onClick={handleOverlayClick}>
        <div class={s.popup}>
          <div class={s.header}>
            <span class={s.title}>MCP Servers</span>
          </div>

          <div class={s.list}>
            <Show when={displayServers().length === 0}>
              <div class={s.empty}>No upstream MCP servers configured</div>
            </Show>
            <For each={displayServers()}>
              {(server) => {
                const st = () => getStatus(server.name);
                const enabled = () => isEnabled(server.name);
                return (
                  <div
                    class={s.item}
                    onClick={() => mcpPopupStore.toggleServer(server.name)}
                  >
                    <span
                      class={s.statusDot}
                      style={{
                        background: enabled()
                          ? STATUS_COLORS[st()?.status ?? "disabled"] ?? "#5c6370"
                          : "#5c6370",
                      }}
                      title={enabled() ? (st()?.status?.replace("_", " ") ?? "unknown") : "disabled"}
                    />
                    <div class={s.info}>
                      <span
                        class={s.name}
                        style={{ opacity: enabled() ? 1 : 0.5 }}
                      >
                        {server.name}
                      </span>
                      <span
                        class={`${s.badge} ${server.transport.type === "http" ? s.badgeHttp : s.badgeStdio}`}
                      >
                        {server.transport.type.toUpperCase()}
                      </span>
                    </div>
                    <span class={s.tools}>
                      {enabled() ? `${st()?.tool_count ?? 0} tools` : "off"}
                    </span>
                    <input
                      type="checkbox"
                      class={s.checkbox}
                      checked={enabled()}
                      onChange={() => mcpPopupStore.toggleServer(server.name)}
                      onClick={(e) => e.stopPropagation()}
                    />
                  </div>
                );
              }}
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
