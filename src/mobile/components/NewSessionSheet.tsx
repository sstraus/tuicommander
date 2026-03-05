import { Show, For } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import styles from "./NewSessionSheet.module.css";

interface NewSessionSheetProps {
  repos: string[];
  onDismiss: () => void;
}

function repoName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

export function NewSessionSheet(props: NewSessionSheetProps) {
  async function createSession(cwd: string) {
    props.onDismiss();
    try {
      await rpc("create_pty", { config: { cwd } });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.warn("network", `Failed to create session: ${msg}`);
    }
  }

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onDismiss();
    }
  };

  return (
    <div class={styles.backdrop} onClick={handleBackdropClick}>
      <div class={styles.sheet}>
        <div class={styles.title}>New Session</div>
        <Show
          when={props.repos.length > 0}
          fallback={<div class={styles.empty}>No repositories configured</div>}
        >
          <For each={props.repos}>
            {(repo) => (
              <button class={styles.repoItem} onClick={() => createSession(repo)}>
                <span class={styles.repoName}>{repoName(repo)}</span>
                <span class={styles.repoPath}>{repo}</span>
              </button>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}
