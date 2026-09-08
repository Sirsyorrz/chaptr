use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::model::Library;
use crate::scan;

pub fn is_present(path: &str) -> bool {
    Path::new(path).is_file()
}

/// Names of the recordings whose files cannot be read from here. A project
/// imported from someone else's machine has all of them missing, and that is a
/// state the app has to work in, not refuse to open.
pub fn missing(lib: &Library) -> Vec<String> {
    lib.recordings
        .iter()
        .filter(|r| !is_present(&r.path))
        .map(|r| r.name.clone())
        .collect()
}

#[derive(Debug, Serialize)]
pub struct Relinked {
    pub linked: usize,
    pub missing: Vec<String>,
    /// Same filename, different bytes. Almost always a re-encode or a proxy,
    /// which would not line up with transcripts made from the original.
    pub mismatched: Vec<String>,
}

/// Points a library at footage that now lives somewhere else, keeping every
/// recording id, so transcripts and chaptrs stay attached.
///
/// Ids are `blake3(filename:size)`, so a candidate can be confirmed rather than
/// trusted: a file only replaces a recording when it reproduces its id.
pub fn relink(lib: &mut Library, dir: &Path) -> Relinked {
    let mut by_id: Vec<(String, PathBuf, u64)> = Vec::new();
    for path in scan::find_videos(dir) {
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        by_id.push((scan::recording_id(&path, meta.len()), path, meta.len()));
    }

    let mut out = Relinked { linked: 0, missing: Vec::new(), mismatched: Vec::new() };
    for rec in &mut lib.recordings {
        if is_present(&rec.path) {
            continue;
        }
        match by_id.iter().find(|(id, ..)| *id == rec.id) {
            Some((_, path, _)) => {
                rec.path = path.to_string_lossy().into_owned();
                out.linked += 1;
            }
            None => {
                let same_name = by_id
                    .iter()
                    .any(|(_, p, _)| p.file_name().map(|n| n == rec.name.as_str()).unwrap_or(false));
                if same_name {
                    out.mismatched.push(rec.name.clone());
                }
                out.missing.push(rec.name.clone());
            }
        }
    }

    lib.root = lib
        .recordings
        .iter()
        .map(|r| r.path.clone())
        .collect::<Vec<_>>()
        .join(", ");
    out
}

/// Sources follow the footage, so a later rescan looks where the files are now
/// rather than at the drive they came from. A source naming one clip is
/// replaced by that clip's new path; anything else becomes the folder the user
/// pointed at.
pub fn remap_sources(lib: &Library, original: &[String], dir: &Path) -> Vec<String> {
    let by_name = |name: &str| {
        lib.recordings
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.path.clone())
    };

    let mut out: Vec<String> = Vec::new();
    for source in original {
        let base = Path::new(source)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mapped = by_name(&base).unwrap_or_else(|| dir.to_string_lossy().into_owned());
        if !out.contains(&mapped) {
            out.push(mapped);
        }
    }
    out
}
