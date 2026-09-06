use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::Serialize;


#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub id: String,
    pub file: String,
    pub received: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Downloads {
    pub cancel: Arc<AtomicBool>,
}

/// Streams a model to disk, reporting as it goes. Several gigabytes over a
/// domestic connection is long enough that silence looks like a hang.
pub fn fetch(
    url: &str,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<u64> {
    let mut received: u64 = 0;

    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }

    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .with_context(|| format!("requesting {url}"))?;

    let total: u64 = resp
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    on_progress(0, total);

    // Download beside the target so a cancelled or failed attempt never leaves
    // something that looks like a usable model.
    let part = dest.with_extension("part");
    let mut out = std::fs::File::create(&part)
        .with_context(|| format!("creating {}", part.display()))?;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 1 << 20];
    let mut last_emit = std::time::Instant::now();

    loop {
        if cancel.load(Ordering::SeqCst) {
            drop(out);
            let _ = std::fs::remove_file(&part);
            bail!("cancelled");
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        received += n as u64;
        if last_emit.elapsed().as_millis() > 200 {
            last_emit = std::time::Instant::now();
            on_progress(received, total);
        }
    }
    out.flush()?;
    drop(out);

    if total > 0 && received < total {
        let _ = std::fs::remove_file(&part);
        bail!("connection closed after {received} of {total} bytes");
    }

    std::fs::rename(&part, dest).with_context(|| format!("finalising {}", dest.display()))?;
    on_progress(received, total);
    Ok(received)
}
