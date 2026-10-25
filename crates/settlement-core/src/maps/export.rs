//! Writing exports to disk: PNG from the rendered SVG, safe file writes and
//! file names. What is written is decided by the service; see
//! `docs/MAPS.md` §15.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use resvg::usvg;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The kinds of map export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Svg,
    Png,
    /// Structured map data (JSON), for other tools.
    Json,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Png => "png",
            ExportFormat::Json => "map.json",
        }
    }
}

/// The largest picture written, in pixels (about 64 megapixels).
pub const MAX_PNG_PIXELS: u64 = 64_000_000;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("The map picture could not be drawn: {0}")]
    Render(String),
    #[error(
        "The map is too large to save as a picture ({width} × {height} pixels). Choose a smaller width."
    )]
    TooLarge { width: u32, height: u32 },
    #[error("Could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The system fonts, loaded once (it takes a moment).
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

/// A PNG of an SVG document drawn by [`super::render::svg`].
pub fn png(svg: &str) -> Result<Vec<u8>, ExportError> {
    let options = usvg::Options {
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_str(svg, &options).map_err(|e| ExportError::Render(e.to_string()))?;
    let size = tree.size().to_int_size();
    let (width, height) = (size.width(), size.height());
    if u64::from(width) * u64::from(height) > MAX_PNG_PIXELS {
        return Err(ExportError::TooLarge { width, height });
    }
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| ExportError::Render("the picture has no size".into()))?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap
        .encode_png()
        .map_err(|e| ExportError::Render(e.to_string()))
}

/// Writes `bytes` to `path` safely: to a temporary file beside it first,
/// flushed to disk, then renamed over the target. An interrupted export
/// leaves either the old file or none, never a half-written one.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let fail = |source| ExportError::Write {
        path: path.to_path_buf(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.partial"));
    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(fail)
}

/// A safe file name from display text (settlement and map names may hold
/// any Unicode): characters Windows does not allow become `_`, reserved
/// device names are avoided, and the length is limited.
pub fn file_name(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" - ");
    let mut out: String = joined
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .take(100)
        .collect();
    out = out.trim_end_matches(['.', ' ']).trim().to_owned();
    if out.is_empty() {
        out = "Settlement".into();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit()));
    if reserved {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe_and_keep_unicode() {
        assert_eq!(
            file_name(&["Ærøskøbing – Ōshū 城", "District 2"]),
            "Ærøskøbing – Ōshū 城 - District 2"
        );
        assert_eq!(file_name(&["a/b:c*?", ""]), "a_b_c__");
        assert_eq!(file_name(&["CON"]), "_CON");
        assert_eq!(file_name(&["  ...  "]), "Settlement");
        assert_eq!(file_name(&[&"x".repeat(300)]).chars().count(), 100);
    }

    #[test]
    fn files_are_written_whole_and_replace_the_old_one() {
        let dir = std::env::temp_dir().join(format!("sg-export-{}", std::process::id()));
        let path = dir.join("map.svg");
        write_file(&path, b"first").unwrap();
        write_file(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(left.len(), 1, "no temporary file is left behind");
        let _ = std::fs::remove_dir_all(dir);
    }
}
