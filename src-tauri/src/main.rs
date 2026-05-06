// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "desktop")]
fn main() {
    tuicommander_lib::run()
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("The desktop GUI requires the 'desktop' feature. Use tuicommander-remote for headless mode.");
    std::process::exit(1);
}
