import { For, Show } from "solid-js";
import type { SessionInfo } from "../useSessions";
import styles from "./QuestionBanner.module.css";

interface QuestionBannerProps {
  sessions: SessionInfo[];
  onNavigate: (sessionId: string) => void;
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return text.slice(0, max - 1) + "\u2026";
}

function BannerItem(props: { session: SessionInfo; onNavigate: (sessionId: string) => void }) {
  return (
    <button
      class={styles.banner}
      onClick={() => props.onNavigate(props.session.session_id)}
    >
      <span class={styles.agent}>
        {props.session.state?.agent_type ?? "Terminal"}
      </span>
      <span class={styles.question}>
        {truncate(props.session.state?.question_text ?? "Awaiting input", 80)}
      </span>
    </button>
  );
}

export function QuestionBanner(props: QuestionBannerProps) {
  const questioners = () =>
    props.sessions.filter((s) => s.state?.awaiting_input);

  return (
    <Show when={questioners().length > 0}>
      <div class={styles.container}>
        <For each={questioners()}>
          {(session) => (
            <BannerItem session={session} onNavigate={props.onNavigate} />
          )}
        </For>
      </div>
    </Show>
  );
}
