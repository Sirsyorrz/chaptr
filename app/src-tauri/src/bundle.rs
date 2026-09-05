use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Library, Transcript};
use crate::workspace::{self, Workspace};

pub const EXTENSION: &str = "chaptr";
const VERSION: u32 = 1;

/// Everything a project is, in one file the user owns and can move or send.
/// Transcripts dominate the size and compress well: 24 hours of footage is
/// about 1.7 MB of JSON, well under half a megabyte gzipped.
#[derive(Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub version: u32,
    pub name: String,
    pub sources: Vec<String>,
    pub library: Option<Library>,
    pub settings: Value,
    pub chaptrs: Value,
    pub edits: Value,
    pub transcripts: BTreeMap<String, Transcript>,
}

fn read_transcripts(ws: &Workspace) -> BTreeMap<String, Transcript> {
    let mut out = BTreeMap::new();
    let dir = ws.root.join("transcripts");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for path in entries.filter_map(|e| e.ok()).map(|e| e.path()) {
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        if let Some(t) = workspace::maybe_json::<Transcript>(&path) {
            out.insert(t.recording_id.clone(), t);
        }
    }
    out
}

/// Older projects store the list under a `beats` key. Bundles always use the
/// current one, so the legacy name cannot spread into new files.
fn chaptr_list(path: &Path) -> Value {
    let Some(v) = workspace::maybe_json::<Value>(path) else {
        return Value::Null;
    };
    let list = if v["chaptrs"].is_array() { &v["chaptrs"] } else { &v["beats"] };
    if list.is_array() {
        serde_json::json!({ "chaptrs": list })
    } else {
        Value::Null
    }
}

pub fn collect(ws: &Workspace, name: &str, sources: &[String]) -> Bundle {
    Bundle {
        version: VERSION,
        name: name.to_string(),
        sources: sources.to_vec(),
        library: workspace::maybe_json(&ws.library()),
        settings: workspace::maybe_json(&ws.settings()).unwrap_or(Value::Null),
        chaptrs: chaptr_list(&ws.chaptrs()),
        edits: chaptr_list(&ws.edits()),
        transcripts: read_transcripts(ws),
    }
}

pub fn save(ws: &Workspace, name: &str, sources: &[String], dest: &Path) -> Result<u64> {
    let bundle = collect(ws, name, sources);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }

    // Write beside the target and rename, so an interrupted save cannot
    // truncate a project file that was already good.
    let tmp = dest.with_extension("chaptr.part");
    {
        let file = std::fs::File::create(&tmp)
            .with_context(|| format!("creating {}", tmp.display()))?;
        let mut gz = GzEncoder::new(file, Compression::default());
        gz.write_all(&serde_json::to_vec(&bundle)?)?;
        gz.finish()?;
    }
    std::fs::rename(&tmp, dest).with_context(|| format!("finalising {}", dest.display()))?;
    Ok(std::fs::metadata(dest)?.len())
}

pub fn read(path: &Path) -> Result<Bundle> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let mut text = String::new();
    GzDecoder::new(file)
        .read_to_string(&mut text)
        .with_context(|| format!("{} is not a chaptr project", path.display()))?;
    let bundle: Bundle = serde_json::from_str(&text)?;
    if bundle.version > VERSION {
        bail!("this project was made by a newer version of chaptr");
    }
    Ok(bundle)
}

/// Expands a bundle into a working directory, which is what the rest of the
/// pipeline reads and writes while a job runs.
pub fn unpack(bundle: &Bundle, ws: &Workspace) -> Result<()> {
    ws.prepare()?;
    if let Some(lib) = &bundle.library {
        workspace::write_json(&ws.library(), lib)?;
    }
    for (key, value) in [
        (ws.root.join("settings.json"), &bundle.settings),
        (ws.root.join("chaptrs.json"), &bundle.chaptrs),
        (ws.root.join("chaptrs.edits.json"), &bundle.edits),
    ] {
        if !value.is_null() {
            workspace::write_json(&key, value)?;
        }
    }
    for (id, transcript) in &bundle.transcripts {
        workspace::write_json(&ws.root.join("transcripts").join(format!("{id}.json")), transcript)?;
    }
    Ok(())
}
