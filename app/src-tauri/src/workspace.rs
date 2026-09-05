use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

/// Everything chaptr derives from a footage folder lives in one directory, so
/// the whole job is one thing to delete, move or back up.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn library(&self) -> PathBuf {
        self.root.join("library.json")
    }
    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn chaptrs(&self) -> PathBuf {
        pick(&self.root, "chaptrs.json", "beats.json")
    }
    pub fn edits(&self) -> PathBuf {
        pick(&self.root, "chaptrs.edits.json", "beats.edits.json")
    }
    pub fn transcript(&self, id: &str) -> PathBuf {
        self.root.join("transcripts").join(format!("{id}.json"))
    }
    pub fn preview(&self, id: &str) -> PathBuf {
        self.root.join("preview").join(format!("{id}.opus"))
    }
    pub fn scratch(&self, id: &str) -> PathBuf {
        self.root.join("scratch").join(format!("{id}.wav"))
    }

    pub fn prepare(&self) -> Result<()> {
        for sub in ["transcripts", "preview", "scratch"] {
            fs::create_dir_all(self.root.join(sub))
                .with_context(|| format!("creating {}", self.root.join(sub).display()))?;
        }
        Ok(())
    }

    pub fn clear_scratch(&self) {
        let _ = fs::remove_dir_all(self.root.join("scratch"));
        let _ = fs::create_dir_all(self.root.join("scratch"));
    }
}

/// Current filename, falling back to the pre-rename one for older projects.
fn pick(root: &Path, current: &str, legacy: &str) -> PathBuf {
    let now = root.join(current);
    let before = root.join(legacy);
    if !now.exists() && before.exists() {
        before
    } else {
        now
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn maybe_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    read_json(path).ok()
}

/// Writes via a temp file so an interrupted run cannot leave a half-written
/// artifact that later stages would treat as complete.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("finalising {}", path.display()))?;
    Ok(())
}
