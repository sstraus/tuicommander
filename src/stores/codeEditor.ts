import { createStore } from "solid-js/store";
import { invoke } from "../invoke";

interface CodeEditorState {
  repoPath: string | null;
  filePath: string | null;
  content: string;
  savedContent: string;
  isDirty: boolean;
  isReadOnly: boolean;
  isLoading: boolean;
  error: string | null;
}

function createCodeEditorStore() {
  const [state, setState] = createStore<CodeEditorState>({
    repoPath: null,
    filePath: null,
    content: "",
    savedContent: "",
    isDirty: false,
    isReadOnly: false,
    isLoading: false,
    error: null,
  });

  const actions = {
    /** Open a file from a repository, loading its content from the backend */
    async openFile(repoPath: string, filePath: string): Promise<void> {
      setState({
        repoPath,
        filePath,
        isLoading: true,
        error: null,
        isDirty: false,
      });

      try {
        const content = await invoke<string>("fs_read_file", { repoPath, file: filePath });
        setState({
          content,
          savedContent: content,
          isLoading: false,
        });
      } catch (err) {
        setState({
          error: String(err),
          isLoading: false,
          content: "",
          savedContent: "",
        });
      }
    },

    /** Update the editor content (marks as dirty if different from saved) */
    setContent(content: string): void {
      setState("content", content);
      setState("isDirty", content !== state.savedContent);
    },

    /** Mark the current content as saved */
    markSaved(): void {
      setState("savedContent", state.content);
      setState("isDirty", false);
    },

    /** Close the current file */
    closeFile(): void {
      setState({
        repoPath: null,
        filePath: null,
        content: "",
        savedContent: "",
        isDirty: false,
        isReadOnly: false,
        isLoading: false,
        error: null,
      });
    },

    /** Toggle read-only mode */
    setReadOnly(readOnly: boolean): void {
      setState("isReadOnly", readOnly);
    },
  };

  return { state, ...actions };
}

export const codeEditorStore = createCodeEditorStore();
