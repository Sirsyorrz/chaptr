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
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }

    // Resume a part-finished download rather than starting an 18 GB file again.
    let part = dest.with_extension("part");
    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

    // An overall timeout is wrong here: it caps the whole transfer, so a large
    // model fails the moment it takes longer than the limit. Bound the connect
    // and each read instead.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(120))
        .build();
    let mut req = agent.get(url);
    if have > 0 {
        req = req.set("range", &format!("bytes={have}-"));
    }
    let resp = req.call().with_context(|| format!("requesting {url}"))?;

    let resuming = resp.status() == 206;
    let remaining: u64 = resp
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let total = if resuming { have + remaining } else { remaining };
    let mut received = if resuming { have } else { 0 };
    on_progress(received, total);

    // Download beside the target so a failed attempt never leaves something
    // that looks like a usable model.
    let mut out = if resuming {
        std::fs::OpenOptions::new().append(true).open(&part)
    } else {
        std::fs::File::create(&part)
    }
    .with_context(|| format!("opening {}", part.display()))?;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 1 << 20];
    let mut last_emit = std::time::Instant::now();

    loop {
        if cancel.load(Ordering::SeqCst) {
            // Keep the partial file: cancelling should not throw away
            // gigabytes the user already waited for.
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
        bail!("connection closed after {received} of {total} bytes; press download again to resume");
    }

    std::fs::rename(&part, dest).with_context(|| format!("finalising {}", dest.display()))?;
    on_progress(received, total);
    Ok(received)
}
