import type { Component } from "solid-js";

/** Globe icon — monochrome, fill="currentColor" per AGENTS.md */
export const GlobeIcon: Component<{ size?: number }> = (props) => (
  <svg viewBox="0 0 16 16" width={props.size ?? 14} height={props.size ?? 14} fill="currentColor">
    <path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0ZM5.37 1.95A6.5 6.5 0 0 0 1.55 7h2.47a12.97 12.97 0 0 1 1.35-5.05ZM4.02 7a11.45 11.45 0 0 1 1.6-5.03C6.4 2.63 7.17 4.49 7.25 7H4.02Zm4.73 0c-.08-2.51-.85-4.37-1.63-5.03A11.45 11.45 0 0 1 8.73 7H11.98Zm2.7 0a12.97 12.97 0 0 0-1.35-5.05A6.5 6.5 0 0 1 14.45 7h-2.47-.53ZM1.55 9a6.5 6.5 0 0 0 3.82 5.05A12.97 12.97 0 0 1 4.02 9H1.55Zm3.97 0c.08 2.51.85 4.37 1.63 5.03A11.45 11.45 0 0 1 5.52 9H7.25Zm1.73 5.03c.78-.66 1.55-2.52 1.63-5.03H8.73A11.45 11.45 0 0 1 7.12 14.03h.13Zm3.2-.08A12.97 12.97 0 0 0 11.98 9h2.47a6.5 6.5 0 0 1-3.82 5.05l-.18-.1Z"/>
  </svg>
);
