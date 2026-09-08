use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::workspace::{self, Workspace};

/// A project is a named set of sources: whole folders, individual clips, or a
/// mix. Derived data never sits next to the footage — it lives here, so a
/// project can be deleted without touching a single video file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub sources: Vec<String>,
    pub created: String,
    pub opened: String,
    #[serde(default)]
    pub recordings: usize,
    #[serde(default)]
    pub duration: f64,
    /// Where the user saved this project, once they have.
    #[serde(default)]
    pub file: Option<String>,
    /// Work exists that is not in the saved file yet.
    #[serde(default)]
    pub unsaved: bool,
}

pub fn data_root() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("chaptr")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/chaptr")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".local/share"))
            .join("chaptr")
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn registry() -> PathBuf {
    data_root().join("projects.json")
}

pub fn workspace(id: &str) -> Workspace {
    Workspace::new(data_root().join("projects").join(id))
}

/// Stable for the same set of sources, so re-opening the same folder finds the
/// work already done rather than starting again.
pub fn id_for(sources: &[String]) -> String {
    let mut sorted: Vec<&str> = sources.iter().map(String::as_str).collect();
    sorted.sort();
    blake3::hash(sorted.join("\u{0}").as_bytes()).to_hex()[..12].to_string()
}

fn nice_name(sources: &[String]) -> String {
    match sources.split_first() {
        None => "empty".into(),
        Some((first, rest)) => {
            let base = Path::new(first)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| first.clone());
            if rest.is_empty() {
                base
            } else {
                format!("{base} +{}", rest.len())
            }
        }
    }
}

pub fn list() -> Vec<Project> {
    workspace::maybe_json(&registry()).unwrap_or_default()
}

fn put(project: Project) -> Result<Project> {
    let mut all = list();
    all.retain(|p| p.id != project.id);
    all.insert(0, project.clone());
    std::fs::create_dir_all(data_root())?;
    workspace::write_json(&registry(), &all)?;
    Ok(project)
}

pub fn open(sources: Vec<String>) -> Result<Project> {
    open_inner(sources, true)
}

/// A bundle carries the source paths of the machine that saved it, which may be
/// another person's drive letters. Everything the project *is* travels inside
/// the file, so importing one must not depend on reaching the footage.
pub fn open_offline(sources: Vec<String>) -> Result<Project> {
    open_inner(sources, false)
}

fn open_inner(sources: Vec<String>, require_media: bool) -> Result<Project> {
    if sources.is_empty() {
        bail!("pick at least one folder or clip");
    }
    if require_media {
        for s in &sources {
            if !Path::new(s).exists() {
                bail!("{s} does not exist");
            }
        }
    }

    let id = id_for(&sources);
    let now = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let existing = list().into_iter().find(|p| p.id == id);

    let project = match existing {
        Some(p) => Project { opened: now, ..p },
        None => Project {
            name: nice_name(&sources),
            id: id.clone(),
            sources,
            created: now.clone(),
            opened: now,
            recordings: 0,
            duration: 0.0,
            file: None,
            unsaved: false,
        },
    };
    workspace(&id).prepare()?;
    put(project)
}

pub fn touch(id: &str, recordings: usize, duration: f64) {
    update(id, |p| {
        p.recordings = recordings;
        p.duration = duration;
    });
}

pub fn update(id: &str, f: impl FnOnce(&mut Project)) {
    let mut all = list();
    if let Some(p) = all.iter_mut().find(|p| p.id == id) {
        f(p);
        let _ = workspace::write_json(&registry(), &all);
    }
}

/// Anything that produces new results marks the project unsaved, so the UI can
/// say so rather than letting hours of work sit only in the working cache.
pub fn mark_unsaved(id: &str) {
    update(id, |p| p.unsaved = true);
}

pub fn put_project(p: Project) -> Result<Project> {
    put(p)
}

pub fn forget(id: &str, delete_data: bool) -> Result<()> {
    let mut all = list();
    all.retain(|p| p.id != id);
    workspace::write_json(&registry(), &all)?;
    if delete_data {
        let _ = std::fs::remove_dir_all(workspace(id).root);
    }
    Ok(())
}

pub fn get(id: &str) -> Option<Project> {
    list().into_iter().find(|p| p.id == id)
}

/// Earlier versions wrote a `.chaptr` directory inside the footage folder.
/// Move it into the project store rather than making the user redo hours of
/// transcription, and leave nothing behind in their video folders.
pub fn adopt_legacy(sources: &[String], id: &str) -> Option<PathBuf> {
    let ws = workspace(id);
    if ws.library().exists() {
        return None;
    }
    for source in sources {
        let legacy = Path::new(source).join(".chaptr");
        if !legacy.join("library.json").exists() {
            continue;
        }
        let _ = std::fs::create_dir_all(ws.root.parent()?);
        let _ = std::fs::remove_dir_all(&ws.root);
        if std::fs::rename(&legacy, &ws.root).is_ok() {
            return Some(legacy);
        }
    }
    None
}
