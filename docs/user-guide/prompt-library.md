# Prompt Library

Store and reuse command templates with variable substitution.

## Access

- **Cmd+K** — Toggle prompt library drawer
- **Toolbar button** — Prompt library icon

## Creating Prompts

1. Open the prompt library (`Cmd+K`)
2. Click "New Prompt"
3. Enter a label (e.g., "Run tests for module")
4. Enter the prompt text with optional variables:

```
cd {{cwd}} && npm test -- --testPathPattern={{module}}
```

5. Save

## Variables

Use `{{variable_name}}` syntax for dynamic values. When you use the prompt, TUI Commander asks you to fill in each variable.

### Built-in Variables

| Variable | Value |
|----------|-------|
| `{{diff}}` | Current git diff |
| `{{changed_files}}` | List of changed files |
| `{{repo_name}}` | Repository name |
| `{{branch}}` | Current branch name |
| `{{cwd}}` | Current working directory |

### Custom Variables

Any `{{name}}` not in the built-in list becomes a custom variable. You'll be prompted to enter a value when using the prompt.

## Using Prompts

1. Open prompt library (`Cmd+K`)
2. Search or browse prompts
3. Click a prompt
4. Fill in any variables
5. Text is injected into the active terminal

## Organization

- **Pin** prompts to keep them at the top
- **Search** by name or content
- **Categories:** Custom, Recent, Favorite
- **Recent list** tracks your last-used prompts

## Run Commands

A simpler alternative for per-branch commands:

- **Cmd+R** — Run the saved command for the active branch
- **Cmd+Shift+R** — Edit the command before running

Configure run commands in Settings → Repository → Scripts tab, or via the "Edit & Run Command" dialog.
