// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "desktop")]
fn main() {
    #[cfg(debug_assertions)]
    tuicommander_lib::dev_backend::record_process_start();
    // Keep generated frontend assets out of the library unit-test dependency graph.
    tuicommander_lib::run(tauri::generate_context!())
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("The desktop GUI requires the 'desktop' feature. Use tuic-remote for headless mode.");
    std::process::exit(1);
}
