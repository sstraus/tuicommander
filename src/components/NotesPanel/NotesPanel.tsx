import { Component, For, Show, createSignal } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
import { notesStore, generateId } from "../../stores/notes";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { getModifierSymbol } from "../../platform";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { formatRelativeTime } from "../../utils/time";
import { appLogger } from "../../stores/appLogger";
import p from "../shared/panel.module.css";
import s from "./NotesPanel.module.css";

export interface NotesPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
  onSendToTerminal: (text: string) => void;
}

const ACCEPTED_IMAGE_TYPES = ["image/png", "image/jpeg", "image/webp", "image/gif"];

/** Map MIME type to file extension */
function mimeToExtension(mime: string): string {
  const map: Record<string, string> = {
    "image/png": "png",
    "image/jpeg": "jpg",
    "image/webp": "webp",
    "image/gif": "gif",
  };
  return map[mime] ?? "png";
}

/** Convert a Blob to a base64 string */
async function blobToBase64(blob: Blob): Promise<string> {
  const buffer = await blob.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

/** Extract last path segment as display name */
function deriveDisplayName(repoPath: string | null): string | null {
  if (!repoPath) return null;
  return repoPath.split("/").filter(Boolean).pop() ?? repoPath;
}

/** Build the text to send to terminal, appending image paths if present */
function buildTerminalText(text: string, images: string[]): string {
  if (images.length === 0) return text;
  return `${text}\n\nAttached images:\n${images.join("\n")}`;
}

export const NotesPanel: Component<NotesPanelProps> = (props) => {
  const [inputText, setInputText] = createSignal("");
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [reassigningId, setReassigningId] = createSignal<string | null>(null);
  const [pendingImages, setPendingImages] = createSignal<string[]>([]);
  const [pendingNoteId, setPendingNoteId] = createSignal<string | null>(null);
  const [editingImages, setEditingImages] = createSignal<string[]>([]);
  let textareaRef: HTMLTextAreaElement | undefined;

  const filteredNotes = () => notesStore.getFilteredNotes(props.repoPath);
  const badgeCount = () => notesStore.filteredCount(props.repoPath);

  const repoOptions = () => {
    const repos = repositoriesStore.state.repositories;
    return Object.entries(repos).map(([path, repo]) => ({
      path,
      displayName: repo.displayName,
    }));
  };

  /** All images for the current input (editing + newly pasted) */
  const allPendingImages = () => [...editingImages(), ...pendingImages()];

  const handlePaste = async (e: ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;

    for (const item of items) {
      if (ACCEPTED_IMAGE_TYPES.includes(item.type)) {
        e.preventDefault();
        const blob = item.getAsFile();
        if (!blob) continue;

        const noteId = pendingNoteId() ?? editingId() ?? generateId();
        if (!pendingNoteId() && !editingId()) setPendingNoteId(noteId);

        try {
          const dataBase64 = await blobToBase64(blob);
          const extension = mimeToExtension(item.type);
          const savedPath = await invoke<string>("save_note_image", {
            noteId,
            dataBase64,
            extension,
          });
          setPendingImages((prev) => [...prev, savedPath]);
        } catch (err) {
          appLogger.error("store", "Failed to save pasted image", err);
        }
        return; // Only handle first image item
      }
    }
    // No image items found — let default text paste proceed
  };

  const handleSubmit = () => {
    const text = inputText();
    const images = allPendingImages();
    if (!text.trim() && images.length === 0) return;

    const editing = editingId();
    if (editing) {
      notesStore.updateNote(editing, text, images);
      setEditingId(null);
    } else {
      const noteId = pendingNoteId() ?? undefined;
      notesStore.addNote(text, props.repoPath, deriveDisplayName(props.repoPath), images, noteId);
    }

    setInputText("");
    setPendingImages([]);
    setPendingNoteId(null);
    setEditingImages([]);
  };

  const handleEdit = (id: string, text: string, images: string[]) => {
    setInputText(text);
    setEditingId(id);
    setEditingImages(images);
    setPendingImages([]);
    setPendingNoteId(null);
    textareaRef?.focus();
  };

  const handleCancelEdit = () => {
    setInputText("");
    setEditingId(null);
    setEditingImages([]);
    setPendingImages([]);
    setPendingNoteId(null);
  };

  const removePendingImage = (path: string) => {
    // Check if it's from editing (existing) or pending (newly pasted)
    if (editingImages().includes(path)) {
      setEditingImages((prev) => prev.filter((p) => p !== path));
    } else {
      setPendingImages((prev) => prev.filter((p) => p !== path));
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
    if (e.key === "Escape" && editingId()) {
      handleCancelEdit();
    }
  };

  const handleReassign = (noteId: string, newRepoPath: string) => {
    if (newRepoPath === "__global__") {
      notesStore.reassignNote(noteId, null, null);
    } else {
      const repos = repositoriesStore.state.repositories;
      const displayName = repos[newRepoPath]?.displayName ?? deriveDisplayName(newRepoPath);
      notesStore.reassignNote(noteId, newRepoPath, displayName);
    }
    setReassigningId(null);
  };

  const handleSend = (note: { text: string; images: string[]; id: string }) => {
    props.onSendToTerminal(buildTerminalText(note.text, note.images));
    notesStore.markUsed(note.id);
  };

  return (
    <div id="notes-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="notes-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}><span style={{ filter: "grayscale(1) brightness(1.5)", "font-style": "normal" }}>💡</span> {t("notesPanel.title", "Ideas")}</span>
          <Show when={badgeCount() > 0}>
            <span class={p.fileCountBadge}>{badgeCount()}</span>
          </Show>
        </div>
        <button class={p.close} onClick={props.onClose} title={`${t("notesPanel.close", "Close")} (${getModifierSymbol()}N)`}>
          &times;
        </button>
      </div>

      <div class={cx(p.content, s.list)}>
        <Show when={filteredNotes().length === 0}>
          <div class={s.empty}>{t("notesPanel.empty", "No ideas yet. Add one below.")}</div>
        </Show>
        <For each={filteredNotes()}>
          {(note) => (
            <div class={cx(s.item, !!note.usedAt && s.itemUsed)}>
              <div class={s.body}>
                <Show when={note.text}>
                  <span class={s.text} title={note.text}>{note.usedAt ? "✓ " : ""}{note.text}</span>
                </Show>
                <Show when={note.images.length > 0}>
                  <div class={note.images.length === 1 && !note.text ? s.singleImageWrap : s.thumbnails}>
                    <For each={note.images}>
                      {(imgPath) => (
                        <img
                          class={note.images.length === 1 && !note.text ? s.singleImage : s.thumbnail}
                          src={convertFileSrc(imgPath)}
                          alt="Note image"
                          loading="lazy"
                          onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
                        />
                      )}
                    </For>
                  </div>
                </Show>
                <div class={s.meta}>
                  <span class={s.date}>{formatRelativeTime(note.createdAt, { showDateFallback: true })}</span>
                  <Show when={reassigningId() === note.id} fallback={
                    <button
                      class={cx(s.projectLabel, note.repoPath ? s.projectTagged : s.projectGlobal)}
                      onClick={() => setReassigningId(note.id)}
                      title="Click to reassign project"
                    >
                      {note.repoDisplayName ?? "Global"}
                    </button>
                  }>
                    <select
                      class={s.reassignSelect}
                      value={note.repoPath ?? "__global__"}
                      onChange={(e) => handleReassign(note.id, e.currentTarget.value)}
                      onBlur={() => setReassigningId(null)}
                      ref={(el) => requestAnimationFrame(() => el.focus())}
                    >
                      <option value="__global__">Global</option>
                      <For each={repoOptions()}>
                        {(repo) => (
                          <option value={repo.path}>{repo.displayName}</option>
                        )}
                      </For>
                    </select>
                  </Show>
                </div>
              </div>
              <div class={s.actions}>
                <button
                  class={cx(s.actionBtn, s.editBtn)}
                  onClick={() => handleEdit(note.id, note.text, note.images)}
                  title={t("notesPanel.edit", "Edit note")}
                >
                  ✎
                </button>
                <button
                  class={cx(s.actionBtn, s.sendBtn)}
                  onClick={() => handleSend(note)}
                  title={t("notesPanel.send", "Send to terminal")}
                >
                  ▶
                </button>
                <button
                  class={cx(s.actionBtn, s.deleteBtn)}
                  onClick={() => notesStore.removeNote(note.id)}
                  title={t("notesPanel.delete", "Delete note")}
                >
                  ✕
                </button>
              </div>
            </div>
          )}
        </For>
      </div>

      <div class={s.inputArea}>
        <Show when={allPendingImages().length > 0}>
          <div class={s.pendingThumbnails}>
            <For each={allPendingImages()}>
              {(imgPath) => (
                <div class={s.pendingThumbWrap}>
                  <img
                    class={s.thumbnail}
                    src={convertFileSrc(imgPath)}
                    alt="Pending image"
                    onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
                  />
                  <button
                    class={s.thumbnailRemove}
                    onClick={() => removePendingImage(imgPath)}
                    title="Remove image"
                  >
                    ✕
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
        <textarea
          ref={textareaRef}
          class={s.input}
          rows={5}
          placeholder={editingId()
            ? t("notesPanel.editPlaceholder", "Edit idea... (Esc to cancel)")
            : t("notesPanel.placeholder", "Type an idea and press Enter... (Ctrl+V to paste image)")}
          value={inputText()}
          onInput={(e) => setInputText(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
        />
        <button
          class={s.submitBtn}
          onClick={handleSubmit}
          disabled={!inputText().trim() && allPendingImages().length === 0}
          title={t("notesPanel.submit", "Add note (Enter)")}
        >
          +
        </button>
      </div>
    </div>
  );
};

export default NotesPanel;
