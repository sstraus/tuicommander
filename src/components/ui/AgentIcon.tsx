import { Component, JSX, Show } from "solid-js";
import type { AgentType } from "../../agents";

export interface AgentIconProps {
  agent: AgentType;
  size?: number;
}

/** SVG path data for agents with official brand icons */
const AGENT_PATHS: Partial<Record<AgentType, { viewBox: string; d: string }>> = {
  // Anthropic "A" lettermark (source: simpleicons.org)
  claude: {
    viewBox: "0 0 24 24",
    d: "M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z",
  },
  // Google Gemini four-pointed sparkle (source: simpleicons.org)
  gemini: {
    viewBox: "0 0 24 24",
    d: "M11.04 19.32Q12 21.51 12 24q0-2.49.93-4.68.96-2.19 2.58-3.81t3.81-2.55Q21.51 12 24 12q-2.49 0-4.68-.93a12.3 12.3 0 0 1-3.81-2.58 12.3 12.3 0 0 1-2.58-3.81Q12 2.49 12 0q0 2.49-.96 4.68-.93 2.19-2.55 3.81a12.3 12.3 0 0 1-3.81 2.58Q2.49 12 0 12q2.49 0 4.68.96 2.19.93 3.81 2.55t2.55 3.81",
  },
  // OpenCode hollow square bracket (source: opencode.ai favicon, normalized to 0 0 24 24)
  opencode: {
    viewBox: "0 0 24 24",
    d: "M18 19.5H6V4.5H18V19.5ZM15 7.5H9V16.5H15V7.5Z",
  },
  // OpenAI hexagonal knot (source: simpleicons.org)
  codex: {
    viewBox: "0 0 24 24",
    d: "M22.282 9.821a5.985 5.985 0 0 0-.516-4.911 6.046 6.046 0 0 0-6.51-2.9A6.065 6.065 0 0 0 4.981 4.18a5.985 5.985 0 0 0-3.998 2.9 6.046 6.046 0 0 0 .743 7.097 5.98 5.98 0 0 0 .51 4.911 6.051 6.051 0 0 0 6.515 2.9A5.985 5.985 0 0 0 13.26 24a6.056 6.056 0 0 0 5.772-4.206 5.99 5.99 0 0 0 3.997-2.9 6.056 6.056 0 0 0-.747-7.073zM13.26 22.43a4.476 4.476 0 0 1-2.876-1.04l.141-.081 4.778-2.758a.795.795 0 0 0 .392-.681v-6.737l2.02 1.168a.071.071 0 0 1 .038.052v5.583a4.504 4.504 0 0 1-4.494 4.494zM3.6 18.304a4.47 4.47 0 0 1-.535-3.014l.142.085 4.783 2.759a.771.771 0 0 0 .78 0l5.843-3.369v2.332a.08.08 0 0 1-.033.062L9.74 19.95a4.5 4.5 0 0 1-6.14-1.646zM2.34 7.896a4.485 4.485 0 0 1 2.366-1.973V11.6a.766.766 0 0 0 .388.676l5.815 3.355-2.02 1.168a.076.076 0 0 1-.071 0l-4.83-2.786A4.504 4.504 0 0 1 2.34 7.872zm16.597 3.855l-5.833-3.387L15.12 7.2a.076.076 0 0 1 .071 0l4.83 2.791a4.494 4.494 0 0 1-.676 8.105v-5.678a.79.79 0 0 0-.407-.667zm2.01-3.023l-.141-.085-4.774-2.782a.776.776 0 0 0-.785 0L9.409 9.23V6.897a.066.066 0 0 1 .028-.061l4.83-2.787a4.5 4.5 0 0 1 6.681 4.66zM8.307 12.863l-2.02-1.164a.08.08 0 0 1-.038-.057V6.074a4.5 4.5 0 0 1 7.376-3.454l-.142.08L8.704 5.46a.795.795 0 0 0-.393.681zm1.098-2.365l2.602-1.5 2.607 1.5v2.999l-2.597 1.5-2.607-1.5z",
  },
};

/** Renders the brand icon for a given AI agent. Falls back to a capital letter when no SVG exists. */
export const AgentIcon: Component<AgentIconProps> = (props): JSX.Element => {
  const size = () => props.size ?? 14;
  const icon = () => AGENT_PATHS[props.agent];

  return (
    <Show
      when={icon()}
      fallback={<span style={{ "font-weight": "700" }}>{props.agent[0].toUpperCase()}</span>}
    >
      {(svg) => (
        <svg
          viewBox={svg().viewBox}
          width={size()}
          height={size()}
          fill="currentColor"
          style={{ "vertical-align": "middle", "flex-shrink": "0" }}
        >
          <path d={svg().d} />
        </svg>
      )}
    </Show>
  );
};
