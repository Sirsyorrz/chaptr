//! Driving DaVinci Resolve's playhead from chaptr.
//!
//! chaptr writes a single line; a Lua script running inside Resolve watches the
//! file and moves the playhead. Deliberately not the scripting API: that is
//! Studio-only from Resolve 19.1 and needs Python, while a script started from
//! Workspace > Scripts works on the free edition with nothing installed.

use std::path::PathBuf;

use anyhow::{Context, Result};

const SCRIPT: &str = include_str!("../../../resolve/Chaptr Link.lua");
const NAME: &str = "Chaptr Link.lua";
/// Earlier builds installed a lowercase name; left behind it shows up as a
/// second, stale entry in Resolve's Scripts menu.
const OLD_NAME: &str = "chaptr link.lua";

/// Where chaptr writes and the Lua script reads. Must match `requestPath()`.
pub fn request_file() -> PathBuf {
    crate::project::data_root().join("goto.txt")
}

/// Every place Resolve looks for user scripts. Resolve's own README documents
/// the Windows path as `%APPDATA%\Roaming\...`, but `%APPDATA%` already ends in
/// Roaming, so both spellings are tried rather than trusting either.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let base = PathBuf::from(appdata);
            for prefix in [base.clone(), base.join("Roaming")] {
                out.push(
                    prefix
                        .join("Blackmagic Design")
                        .join("DaVinci Resolve")
                        .join("Support")
                        .join("Fusion")
                        .join("Scripts")
                        .join("Utility"),
                );
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            out.push(PathBuf::from(home).join(".local/share/DaVinciResolve/Fusion/Scripts/Utility"));
        }
    }
    out
}

/// Prefer a folder Resolve has actually created; otherwise the first candidate,
/// which is what gets created on install.
pub fn scripts_dir() -> Option<PathBuf> {
    let all = candidates();
    all.iter()
        .find(|d| d.exists())
        .cloned()
        .or_else(|| all.first().cloned())
}

pub fn installed() -> bool {
    candidates().iter().any(|d| d.join(NAME).exists())
}

/// Copy the watcher into Resolve's scripts folder. Resolve picks it up without
/// a restart, but it has to be started by hand once per session.
pub fn install() -> Result<PathBuf> {
    let dir = scripts_dir().context("could not work out Resolve's scripts folder")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let dest = dir.join(NAME);
    std::fs::write(&dest, SCRIPT).with_context(|| format!("writing {}", dest.display()))?;
    for d in candidates() {
        let _ = std::fs::remove_file(d.join(OLD_NAME));
    }
    Ok(dest)
}

/// Ask Resolve to seek. The counter makes a repeated click on the same moment
/// a new request, which a bare timestamp would not.
pub fn goto(file: &str, offset: f64) -> Result<()> {
    let path = request_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let n = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.split('\t').next().and_then(|n| n.parse::<u64>().ok()))
        .unwrap_or(0)
        + 1;

    let name = std::path::Path::new(file)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.to_string());

    std::fs::write(&path, format!("{n}\t{name}\t{offset:.3}"))?;
    Ok(())
}
