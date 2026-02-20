use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{App, Wry};

/// Build the native system menu bar.
///
/// Custom items use string IDs (e.g. "new-tab") that are emitted to the frontend
/// via `app.emit("menu-action", id)` so the JS side can dispatch to the same
/// handlers used by keyboard shortcuts.
pub fn build_menu(app: &App) -> Result<tauri::menu::Menu<Wry>, tauri::Error> {
    let is_macos = cfg!(target_os = "macos");

    // ---------- helpers for custom items with accelerators ----------
    macro_rules! item {
        ($id:expr, $label:expr, $accel:expr) => {
            MenuItemBuilder::with_id($id, $label)
                .accelerator($accel)
                .build(app)?
        };
        ($id:expr, $label:expr) => {
            MenuItemBuilder::with_id($id, $label).build(app)?
        };
    }

    // ---------- File ----------
    let mut file = SubmenuBuilder::new(app, "&File");
    file = file
        .item(&item!("new-tab", "New Tab", "CmdOrCtrl+T"))
        .item(&item!("close-tab", "Close Tab", "CmdOrCtrl+W"))
        .item(&item!(
            "reopen-closed-tab",
            "Reopen Closed Tab",
            "CmdOrCtrl+Shift+T"
        ))
        .separator()
        .item(&item!("settings", "Settings", "CmdOrCtrl+,"));

    if !is_macos {
        // On Windows/Linux, Quit lives in File menu
        file = file
            .separator()
            .item(&PredefinedMenuItem::quit(app, None)?);
    }
    let file = file.build()?;

    // ---------- Edit (predefined OS items) ----------
    let edit = SubmenuBuilder::new(app, "&Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .separator()
        .item(&item!(
            "clear-terminal",
            "Clear Terminal",
            "CmdOrCtrl+L"
        ))
        .build()?;

    // ---------- View ----------
    let view = SubmenuBuilder::new(app, "&View")
        .item(&item!(
            "toggle-sidebar",
            "Toggle Sidebar",
            "CmdOrCtrl+["
        ))
        .separator()
        .item(&item!("split-right", "Split Right", "CmdOrCtrl+\\"))
        .item(&item!(
            "split-down",
            "Split Down",
            "CmdOrCtrl+Alt+\\"
        ))
        .separator()
        .item(&item!("zoom-in", "Zoom In", "CmdOrCtrl+="))
        .item(&item!("zoom-out", "Zoom Out", "CmdOrCtrl+-"))
        .item(&item!("zoom-reset", "Reset Zoom", "CmdOrCtrl+0"))
        .separator()
        .item(&item!("diff-panel", "Diff Panel", "CmdOrCtrl+D"))
        .item(&item!(
            "markdown-panel",
            "Markdown Panel",
            "CmdOrCtrl+M"
        ))
        .item(&item!(
            "notes-panel",
            "Notes Panel",
            "CmdOrCtrl+N"
        ))
        .build()?;

    // ---------- Go ----------
    let mut go = SubmenuBuilder::new(app, "&Go");
    go = go
        .item(&item!(
            "next-tab",
            "Next Tab",
            "CmdOrCtrl+Shift+]"
        ))
        .item(&item!(
            "prev-tab",
            "Previous Tab",
            "CmdOrCtrl+Shift+["
        ))
        .separator();

    // Tab 1-9 shortcuts
    for i in 1..=9u8 {
        go = go.item(&item!(
            format!("switch-tab-{i}"),
            format!("Switch to Tab {i}"),
            format!("CmdOrCtrl+{i}")
        ));
    }
    let go = go.build()?;

    // ---------- Tools ----------
    let tools = SubmenuBuilder::new(app, "&Tools")
        .item(&item!(
            "prompt-library",
            "Prompt Library",
            "CmdOrCtrl+K"
        ))
        .item(&item!(
            "run-command",
            "Run Command",
            "CmdOrCtrl+R"
        ))
        .item(&item!(
            "edit-run-command",
            "Edit && Run Command",
            "CmdOrCtrl+Shift+R"
        ))
        .separator()
        .item(&item!("lazygit", "Lazygit", "CmdOrCtrl+G"))
        .item(&item!(
            "lazygit-split",
            "Lazygit Split",
            "CmdOrCtrl+Shift+L"
        ))
        .item(&item!(
            "git-operations",
            "Git Operations",
            "CmdOrCtrl+Shift+G"
        ))
        .separator()
        .item(&item!("task-queue", "Task Queue", "CmdOrCtrl+J"))
        .build()?;

    // ---------- Help ----------
    let mut help = SubmenuBuilder::new(app, "&Help");
    help = help
        .item(&item!("help-panel", "Help Panel", "CmdOrCtrl+?"))
        .separator();
    if !is_macos {
        help = help.item(&item!("check-for-updates", "Check for Updates…"));
    }
    let help = help
        .item(&item!("about", "About TUI Commander"))
        .build()?;

    // ---------- Assemble ----------
    let mut menu = MenuBuilder::new(app);

    if is_macos {
        // macOS: App menu with standard items
        let app_menu = SubmenuBuilder::new(app, "TUI Commander")
            .item(&PredefinedMenuItem::about(app, Some("About TUI Commander"), None)?)
            .separator()
            .item(&item!("check-for-updates", "Check for Updates…"))
            .separator()
            .item(&PredefinedMenuItem::services(app, None)?)
            .separator()
            .item(&PredefinedMenuItem::hide(app, None)?)
            .item(&PredefinedMenuItem::hide_others(app, None)?)
            .item(&PredefinedMenuItem::show_all(app, None)?)
            .separator()
            .item(&PredefinedMenuItem::quit(app, None)?)
            .build()?;
        menu = menu.item(&app_menu);
    }

    let menu = menu
        .item(&file)
        .item(&edit)
        .item(&view)
        .item(&go)
        .item(&tools)
        .item(&help)
        .build()?;

    Ok(menu)
}

#[cfg(test)]
mod tests {
    // Smoke test: verify the module compiles and the function signature is correct.
    // Full menu building requires a Tauri App handle, which is only available
    // at runtime, so we can't unit-test build_menu() without an integration harness.
    #[test]
    fn module_compiles() {
        // If this test runs, the menu module compiles correctly.
        assert!(true);
    }
}
