import { Component, createEffect, createSignal } from "solid-js";
import { DiffViewer } from "../ui";
import { useRepository } from "../../hooks/useRepository";

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

  // Load file diff when props change
  createEffect(() => {
    const repoPath = props.repoPath;
    const filePath = props.filePath;
    const scope = props.scope;

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
    <div class="diff-tab-content">
      <DiffViewer
        diff={diff()}
        emptyMessage={
          loading()
            ? "Loading diff..."
            : error()
              ? `Error: ${error()}`
              : "No changes"
        }
      />
    </div>
  );
};

export default DiffTab;
