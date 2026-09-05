use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// External executables chaptr drives. Shipped alongside the app in `bin/`,
/// with a fall back to PATH so a dev checkout works without vendoring them.
#[derive(Debug, Clone)]
pub struct Sidecars {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub whisper: PathBuf,
    pub llama: PathBuf,
}

fn exe(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn bundled_dir() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.join("bin");
    dir.is_dir().then_some(dir)
}

fn on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn locate(stem: &str) -> Option<PathBuf> {
    let name = exe(stem);
    if let Some(dir) = bundled_dir() {
        let p = dir.join(&name);
        if p.is_file() {
            return Some(p);
        }
    }
    on_path(&name)
}

impl Sidecars {
    pub fn discover() -> Self {
        let miss = |s: &str| PathBuf::from(exe(s));
        Self {
            ffmpeg: locate("ffmpeg").unwrap_or_else(|| miss("ffmpeg")),
            ffprobe: locate("ffprobe").unwrap_or_else(|| miss("ffprobe")),
            whisper: locate("whisper-cli").unwrap_or_else(|| miss("whisper-cli")),
            llama: locate("llama-server").unwrap_or_else(|| miss("llama-server")),
        }
    }

    /// Which of the required tools are missing, by display name.
    pub fn missing(&self) -> Vec<String> {
        [
            ("ffmpeg", &self.ffmpeg),
            ("ffprobe", &self.ffprobe),
            ("whisper-cli", &self.whisper),
            ("llama-server", &self.llama),
        ]
        .iter()
        .filter(|(_, p)| !p.is_file() && on_path(&p.to_string_lossy()).is_none())
        .map(|(n, _)| n.to_string())
        .collect()
    }

    pub fn require(&self, tool: &Path) -> Result<()> {
        if tool.is_file() || on_path(&tool.to_string_lossy()).is_some() {
            return Ok(());
        }
        bail!("{} not found; install it or drop it in the app's bin/ folder", tool.display())
    }
}
