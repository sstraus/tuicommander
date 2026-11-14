# Prompt input keyboard inventory

Source audit: 2026-11-02, base `dbefa5be83e5a5d791c6d6ac7a1fafed3be6acc5`.
This is the current behavior, not an implemented new model.
Boss limits the save shortcut to prompt inputs. Search fields, names, settings
fields and the terminal keyboard transport are outside the change.

| Input / owner | Kind | Send / save | Newline | Cancel |
| --- | --- | --- | --- | --- |
| `ComposePanel/ComposePanel.tsx` | CodeMirror prompt, desktop CLI and Chat | Enter sends; Shift+Ctrl+Enter queues; no Save chord | Ctrl+Enter or Shift+Enter | Escape closes through host callback |
| `AIChatPanel/Composer.tsx` | Prompt textarea, desktop and mobile AI Chat | Enter sends (including Ctrl+Enter); Ctrl+S parks/swaps durable draft; Cmd+S unhandled | Shift+Enter | Escape dismisses command completion when open |
| `mobile/components/CommandInput.tsx` | Terminal prompt textarea, mobile | Enter sends (including Ctrl+Enter); no Save chord | Shift+Enter | Clears draft and sends native Escape; atomic reply only clears |
| `shared/SmartPromptEditor.tsx`, drawer adapter | Prompt content/system prompt textareas, metadata inputs | Explicit Save button in `PromptDrawer`; no Save chord | Native multiline editing | Escape closes drawer editor |
| `shared/SmartPromptEditor.tsx`, Settings adapter | Same fields | Each change calls `promptLibraryStore.updatePrompt`; no explicit Save | Native multiline editing | Host navigation |
| `AutofixDialog/AutofixDialog.tsx` | Editable launch prompt textarea | Start auto-fix button; no Save chord | Native multiline editing | Escape via modal stack |
| `SmartPromptsDropdown/VariableInputDialog.tsx` | Single-line prompt variables | Enter submits form when all fields filled | None | Escape via modal stack |
| `PromptDrawer/PromptDrawer.tsx`, variable dialog | Single-line prompt variables | Insert / Insert and Run buttons | None | Escape closes variable dialog |

## Shared model

Enter and Ctrl+Enter send in desktop Compose, AI Chat (desktop and mobile), and
the mobile terminal composer. Shift+Enter adds a newline. Compose retains its
agent-only Shift+Ctrl+Enter queue shortcut. Ctrl+Enter now follows the two other
composers instead of retaining Compose's earlier newline behavior.

Ctrl+S or Cmd+S parks the current draft and clears the composer. Press it with an
empty composer to restore the parked draft, or with another draft to swap them.
Save never sends. Both modifiers are accepted, preserving existing Control-key
use on macOS while adding the usual Command-key shortcut. Repeated Save keydown
and IME composition do not park or send. Alt-modified shortcuts are untouched.

`src/utils/promptKeybinding.ts` owns classification. `src/stores/promptDraft.ts`
is the shared draft owner. AI Chat keeps its existing session storage keys and
attachments. Terminal Compose and mobile use a shared PTY-keyed owner, with
`terminal:` storage keys. Draft parking is separate from the host's command FIFO.
The existing IndexedDB storage failure signal remains available on the owner;
AI Chat retains its warning UI. Save handlers are local to focused inputs;
the terminal keyboard transport is unchanged (Ctrl+S there remains XOFF).

<!-- prompt-composer: src/components/ComposePanel/ComposePanel.tsx -->
<!-- prompt-composer: src/components/AIChatPanel/Composer.tsx -->
<!-- prompt-composer: src/mobile/components/CommandInput.tsx -->

## Scope

The inventory above records the pre-change behavior of inspected prompt owners.
The shared composer model applies to the three active send composers identified
above. Saved prompt template fields, prompt variables and launch-preview forms
retain their existing save/confirm operations and native multiline editing.
Generic text fields, automation configuration, search, filenames, and the PTY
keyboard transport are outside Boss's prompt-composer correction.
