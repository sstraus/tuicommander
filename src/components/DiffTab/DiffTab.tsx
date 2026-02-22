import { Component, createEffect, createSignal } from "solid-js";
import { DiffViewer } from "../ui";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { t } from "../../i18n";
import s from "./DiffTab.module.css";

export interface DiffTabProps {
  repoPath: string;
  filePath: string;
  scope?: string;
  onClose?: () => void;
}

export const DiffTab: Component<DiffTabProps> = (props) => {
  const [diff, setDiff] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();

  // Load file diff when props change or the repo revision bumps (git index/HEAD changed)
  createEffect(() => {
    const repoPath = props.repoPath;
    const filePath = props.filePath;
    const scope = props.scope;
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

    if (!repoPath || !filePath) {
      setDiff("");
      return;
    }

    setLoading(true);
    setError(null);

    (async () => {
      try {
        const diffContent = await repo.getFileDiff(repoPath, filePath, scope);
        setDiff(diffContent);
      } catch (err) {
        setError(String(err));
        setDiff("");
      } finally {
        setLoading(false);
      }
    })();
  });

  return (
    <div class={s.content}>
      <DiffViewer
        diff={diff()}
        emptyMessage={
          loading()
            ? t("diffTab.loading", "Loading diff...")
            : error()
              ? `${t("diffTab.error", "Error:")} ${error()}`
              : t("diffTab.noChanges", "No changes")
        }
      />
    </div>
  );
};

export default DiffTab;
