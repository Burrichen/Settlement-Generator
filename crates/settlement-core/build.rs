//! Embeds every `content/*.json` file so both binaries ship identical bundled
//! defaults. The file list is discovered here; no content lives in Rust.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let content = manifest.join("..").join("..").join("content");
    println!("cargo:rerun-if-changed={}", content.display());

    let mut files: Vec<PathBuf> = fs::read_dir(&content)
        .expect("content directory exists")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();

    let mut out = String::from("pub(crate) const BUNDLED_FILES: &[(&str, &str)] = &[\n");
    for file in &files {
        let name = file.file_name().expect("file name").to_string_lossy();
        out.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            file.display().to_string()
        ));
    }
    out.push_str("];\n");
    let dest = PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("bundled_files.rs");
    fs::write(dest, out).expect("write bundled file list");
}
