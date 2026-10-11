//! Embeds every `.json` file under `content/` (including subfolders such as
//! `content/kurovia/naming/`) so both binaries ship identical bundled
//! defaults. The file list is discovered here; no content lives in Rust.
//! Files are merged in order of their path relative to `content/`.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .expect("content directory exists")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let relative = path
                .strip_prefix(root)
                .expect("under content/")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((relative, path));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let content = manifest.join("..").join("..").join("content");
    // A directory is scanned recursively for changes.
    println!("cargo:rerun-if-changed={}", content.display());

    let mut files = Vec::new();
    collect(&content, &content, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::from("pub(crate) const BUNDLED_FILES: &[(&str, &str)] = &[\n");
    for (name, file) in &files {
        out.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            file.display().to_string()
        ));
    }
    out.push_str("];\n");
    let dest = PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("bundled_files.rs");
    fs::write(dest, out).expect("write bundled file list");
}
