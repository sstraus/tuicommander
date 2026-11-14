//! Generates the design-mode extension asset list from the extension directory,
//! so a file the extension imports cannot be left out of a second, hand-kept list
//! (story 3045-df36: worker.js imported consent.js, which was never deployed).
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let extension = manifest.join("../../../browser-extension/design-mode");
    // Rerun when a file is added or removed; content edits are tracked through
    // the `include_str!` dep-info of the generated file.
    println!("cargo:rerun-if-changed={}", extension.display());

    let mut assets: Vec<(String, PathBuf)> = std::fs::read_dir(&extension)
        .unwrap_or_else(|e| panic!("read {}: {e}", extension.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            // Type declarations are for the editor, not the browser.
            (!name.ends_with(".d.ts")).then_some((name, path))
        })
        .collect();
    // The page extractor lives with the backend that also injects it.
    assets.push((
        "extract.js".to_owned(),
        manifest.join("../../src/design_mode/extract.js"),
    ));
    assets.sort();

    let mut source = String::from("pub(crate) const ASSETS: &[(&str, &str)] = &[\n");
    for (name, path) in &assets {
        source.push_str(&format!("    ({name:?}, include_str!({path:?})),\n"));
    }
    source.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("native_assets.rs");
    std::fs::write(out, source).unwrap();
}
