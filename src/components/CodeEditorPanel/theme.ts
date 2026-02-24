import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";

/** CodeMirror 6 theme using the app's CSS variables for consistent look */
const editorTheme = EditorView.theme(
  {
    "&": {
      width: "100%",
      height: "100%",
      fontSize: "13px",
    },
    ".cm-scroller": {
      fontFamily: "var(--font-mono)",
      overflow: "auto",
    },
    ".cm-content": {
      caretColor: "var(--accent)",
      padding: "8px 0",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--accent)",
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
      {
        backgroundColor: "rgba(122, 162, 247, 0.2)",
      },
    ".cm-activeLine": {
      backgroundColor: "rgba(255, 255, 255, 0.04)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--bg-tertiary)",
      color: "var(--fg-muted)",
      border: "none",
      borderRight: "1px solid var(--border)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgba(255, 255, 255, 0.06)",
      color: "var(--fg-secondary)",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      padding: "0 8px 0 16px",
      minWidth: "3ch",
    },
    ".cm-matchingBracket": {
      backgroundColor: "rgba(122, 162, 247, 0.25)",
      outline: "1px solid rgba(122, 162, 247, 0.5)",
    },
    ".cm-searchMatch": {
      backgroundColor: "rgba(224, 175, 104, 0.3)",
    },
    ".cm-searchMatch.cm-searchMatch-selected": {
      backgroundColor: "rgba(224, 175, 104, 0.5)",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--bg-secondary)",
      border: "1px solid var(--border)",
      color: "var(--fg-primary)",
    },
  },
  { dark: true },
);

/** Syntax highlighting colors */
const highlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: "#bb9af7" },
  { tag: tags.controlKeyword, color: "#bb9af7" },
  { tag: tags.operator, color: "#89ddff" },
  { tag: tags.punctuation, color: "#a9b1d6" },
  { tag: tags.string, color: "#9ece6a" },
  { tag: tags.regexp, color: "#e0af68" },
  { tag: tags.number, color: "#ff9e64" },
  { tag: tags.bool, color: "#ff9e64" },
  { tag: tags.null, color: "#ff9e64" },
  { tag: tags.comment, color: "#565f89", fontStyle: "italic" },
  { tag: tags.lineComment, color: "#565f89", fontStyle: "italic" },
  { tag: tags.blockComment, color: "#565f89", fontStyle: "italic" },
  { tag: tags.function(tags.variableName), color: "#7aa2f7" },
  { tag: tags.definition(tags.variableName), color: "#c0caf5" },
  { tag: tags.variableName, color: "#c0caf5" },
  { tag: tags.typeName, color: "#2ac3de" },
  { tag: tags.className, color: "#2ac3de" },
  { tag: tags.propertyName, color: "#73daca" },
  { tag: tags.tagName, color: "#f7768e" },
  { tag: tags.attributeName, color: "#bb9af7" },
  { tag: tags.heading, color: "#7aa2f7", fontWeight: "bold" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, fontWeight: "bold" },
  { tag: tags.link, color: "#7aa2f7", textDecoration: "underline" },
]);

/** Combined theme extension for the code editor */
export const codeEditorTheme: Extension = [
  editorTheme,
  syntaxHighlighting(highlightStyle),
];
