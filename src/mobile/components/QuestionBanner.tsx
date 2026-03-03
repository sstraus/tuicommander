import { createEffect, createSignal, For, Show } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import type { SessionInfo } from "../useSessions";
import styles from "./QuestionBanner.module.css";

interface QuestionBannerProps {
  sessions: SessionInfo[];
  onNavigate: (sessionId: string) => void;
}

interface BannerItemProps {
  session: SessionInfo;
  onNavigate: (sessionId: string) => void;
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return text.slice(0, max - 1) + "\u2026";
}

async function sendReply(sessionId: string, text: string) {
  try {
    await rpc("write_pty", { sessionId, data: text + "\n" });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    appLogger.warn("network", `Failed to send reply: ${msg}`);
  }
}

function BannerItem(props: BannerItemProps) {
  const [sent, setSent] = createSignal(false);

  // Reset sent state when awaiting_input clears (next poll)
  createEffect(() => {
    if (!props.session.state?.awaiting_input) {
      setSent(false);
    }
  });

  function handleReply(answer: string) {
    if (sent()) return;
    setSent(true);
    void sendReply(props.session.session_id, answer);
  }

  return (
    <div class={styles.banner}>
      <button
        class={styles.content}
        onClick={() => props.onNavigate(props.session.session_id)}
      >
        <span class={styles.agent}>
          {props.session.state?.agent_type ?? "Terminal"}
        </span>
        <span class={styles.question}>
          {truncate(props.session.state?.question_text ?? "Awaiting input", 80)}
        </span>
      </button>
      <div class={styles.actions}>
        <button
          class={styles.yesBtn}
          disabled={sent()}
          onClick={() => handleReply("yes")}
        >
          {sent() ? "Sent" : "Yes"}
        </button>
        <button
          class={styles.noBtn}
          disabled={sent()}
          onClick={() => handleReply("no")}
        >
          {sent() ? "Sent" : "No"}
        </button>
      </div>
    </div>
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
