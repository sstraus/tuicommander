# Smart Prompts Library

The Smart Prompts Library stores and manages all your prompt templates — both built-in AI automation prompts and your custom templates. Prompts are injected directly into the active agent terminal or run headless for quick one-shot operations.

> **Looking for one-click AI automation?** See [Smart Prompts](smart-prompts.md) for the full guide on built-in automation prompts, context variables, and headless execution.

Search for **Configure** to find the three [guided event configuration prompts](event-driven-templates.md): issue to draft PR, incoming PR review and authorized bounded fix. They start in the composer for review; they are not scheduled Automation templates or event subscriptions.

## Opening the Drawer

- **Cmd+Shift+K** — Toggle the prompt library drawer
- **Toolbar button** — Prompt library icon in the main toolbar

## Browsing and Searching

When the drawer opens, the search input is focused automatically. Type to filter prompts by name, description, or content. Matching is case-insensitive and searches all three fields simultaneously.

Use the category tabs to narrow the list:

| Tab | Shows |
|-----|-------|
| **All** | Every saved prompt, sorted by most recently used |
| **Custom** | User-created prompts |
| **Favorites** | Prompts you have starred |
| **Recent** | Last 10 prompts you used |

## Keyboard Navigation

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move selection up/down |
| `Enter` | Run the selected prompt using its Auto-execute setting |
| Double-click | Insert and submit exactly once |
| `Ctrl+N` / `Cmd+N` | Create a new prompt |
| `Ctrl+E` / `Cmd+E` | Edit the selected prompt |
| `Ctrl+F` / `Cmd+F` | Toggle favorite on the selected prompt |
| `Escape` | Close the drawer |

## Creating a Prompt

1. Open the drawer (`Cmd+Shift+K`) and click **+ New Prompt**, or press `Ctrl+N`/`Cmd+N`
2. Fill in the fields:
   - **Name** (required) — shown in the list
   - **Description** — optional subtitle, also searchable
   - **Content** (required) — the text to insert; use `{{variable}}` for dynamic values
   - **Auto-execute** — submit immediately, or leave the inserted text editable for review
   - **Keyboard Shortcut** — optional global shortcut to trigger this prompt directly
3. Click **Save**

## Editing and Deleting

The drawer and **Settings > Smart Prompts** use the same aligned editor. Placement choices have readable names and include **File menu**. Execution mode controls which target, agent and output fields appear. **Send immediately** appears only for injection directly into the terminal.

In the drawer, **Save** applies your draft and **Cancel** discards it. Settings applies each edit immediately. Changing the execution mode does not convert prompt content: use **Shell script (direct run)** only for shell commands, not natural-language instructions.

- Click the **pencil icon** on any prompt row, or select it and press `Ctrl+E`/`Cmd+E`
- Click the **trash icon** to delete — a confirmation dialog appears before deletion

## Variable Substitution

Use `{{variable_name}}` placeholders in prompt content. When you send a prompt that contains variables, a dialog appears asking you to fill in each value before injection.

```
cd {{project_dir}} && cargo test -- {{test_filter}}
```

### Built-in Variables

These are resolved automatically by the backend when present:

| Variable | Value |
|----------|-------|
| `{{diff}}` | Current git diff |
| `{{changed_files}}` | List of changed files |
| `{{repo_name}}` | Repository name |
| `{{branch}}` | Current branch name |
| `{{cwd}}` | Current working directory |

### Custom Variables

Any `{{name}}` not in the built-in list becomes a custom input field in the variable dialog. You can optionally add a description and default value per variable when editing the prompt — the description appears as placeholder text in the dialog.

### Inserting with Variables

The variable dialog offers two actions:

- **Insert** — writes the resolved text to the terminal input line (you can review before pressing Enter)
- **Insert & Run** — submits the resolved text once through the agent-aware command path

These explicit actions override the saved Auto-execute setting for that use.

## Favorites and Pinning

Click the **star icon** on any prompt row to toggle its favorite status. Favorited prompts appear at the top of any list view with a `★` prefix and are accessible via the **Favorites** category tab.

## Recently Used

The **Recent** tab shows the last 10 prompts you sent, in order of use. Recency is also used to sort the **All** view — most recently used prompts appear first.

## Sending to Terminal

Selecting a prompt (click or Enter) writes its content to the currently active terminal. Auto-execute submits it immediately; otherwise the text remains editable. If the prompt has variables, the variable dialog appears first and its **Insert** or **Insert & Run** choice takes precedence.

The drawer closes automatically after a successful injection and focus returns to the terminal.

---

## Run Commands

A lighter-weight alternative for per-branch one-off commands:

- **Cmd+Shift+R** — Run the saved command for the active branch
- **Cmd+R** — Reload the active web/preview tab

Configure run commands in **Settings → Repository → Scripts** tab.
