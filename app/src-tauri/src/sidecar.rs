use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};

/// Builds a command for one of the bundled tools.
///
/// The whisper.cpp and llama.cpp Linux builds are dynamically linked against
/// their own `.so` files sitting next to them, which the loader will not find
/// on its own. Pointing it at the tool's own directory avoids having to
/// rewrite rpaths at packaging time.
pub fn command(tool: &Path) -> Command {
    let mut cmd = Command::new(tool);
    if cfg!(target_os = "linux") {
        if let Some(dir) = tool.parent().filter(|d| !d.as_os_str().is_empty()) {
            let mut dirs = vec![dir.to_path_buf()];
            if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
                dirs.extend(std::env::split_paths(&existing));
            }
            if let Ok(joined) = std::env::join_paths(dirs) {
                cmd.env("LD_LIBRARY_PATH", joined);
            }
        }
    }
    cmd
}

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

/// Where a packaged build keeps its executables. Windows and macOS put
/// resources beside the binary; deb and AppImage put the binary in `bin/` and
/// its resources in `lib/<product>/`, so both shapes are tried.
fn bundled_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("CHAPTR_BIN") {
        out.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(here) = exe.parent() {
            out.push(here.join("bin"));
            if let Some(up) = here.parent() {
                // deb and AppImage key the resource folder off the product
                // name, which is not the binary name: /usr/bin/chaptr-app has
                // its resources in /usr/lib/chaptr/bin.
                out.push(up.join("lib").join(env!("CARGO_PKG_NAME")).join("bin"));
                if let Some(stem) = exe.file_stem() {
                    out.push(up.join("lib").join(stem).join("bin"));
                }
                out.push(up.join("Resources").join("bin"));
            }
        }
    }
    out.retain(|d| d.is_dir());
    out
}

fn on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn locate(stem: &str) -> Option<PathBuf> {
    let name = exe(stem);
    for dir in bundled_dirs() {
        let direct = dir.join(&name);
        if direct.is_file() {
            return Some(direct);
        }
        // Each tool sits in its own subfolder with its own libraries.
        let subdirs = std::fs::read_dir(&dir).into_iter().flatten().flatten();
        for entry in subdirs {
            let candidate = entry.path().join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
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
