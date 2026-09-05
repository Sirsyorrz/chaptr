use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub struct ClipSummary {
    key: String,
    name: String,
    duration: f64,
    fps: f64,
    wall_start: String,
    session_id: i64,
    markers: usize,
    has_proxy: bool,
    edited: bool,
}

#[derive(Serialize)]
pub struct ClipDetail {
    key: String,
    clip: Value,
    markers: Vec<Value>,
    peaks: Value,
    proxy: Option<String>,
    edited: bool,
}

#[derive(Deserialize)]
pub struct Workspace {
    root: String,
}

fn root_of(ws: &str) -> PathBuf {
    PathBuf::from(shellexpand(ws))
}

fn shellexpand(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

fn read_json(path: &Path) -> Option<Value> {
    fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok())
}

fn markers_for(root: &Path, key: &str) -> (Vec<Value>, bool) {
    let edited = root.join("markers").join(format!("{key}.edited.json"));
    let plain = root.join("markers").join(format!("{key}.json"));
    let is_edited = edited.exists();
    let src = if is_edited { edited } else { plain };
    let list = read_json(&src)
        .and_then(|v| v.get("markers").cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    (list, is_edited)
}

#[tauri::command]
fn list_clips(ws: Workspace) -> Result<Vec<ClipSummary>, String> {
    let root = root_of(&ws.root);
    let dir = root.join("clips");
    if !dir.exists() {
        return Err(format!("no clips directory in {}", root.display()));
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    entries.sort();

    for path in entries {
        let key = path.file_stem().unwrap().to_string_lossy().into_owned();
        let Some(data) = read_json(&path) else { continue };
        let (markers, edited) = markers_for(&root, &key);
        let clip_path = data.get("clip").and_then(|v| v.as_str()).unwrap_or("");
        out.push(ClipSummary {
            name: Path::new(clip_path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| key.clone()),
            duration: data.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
            fps: data.get("fps").and_then(|v| v.as_f64()).unwrap_or(60.0),
            wall_start: data
                .get("wall_start")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            session_id: data.get("session_id").and_then(|v| v.as_i64()).unwrap_or(-1),
            markers: markers.len(),
            has_proxy: root.join("proxy").join(format!("{key}.mp4")).exists(),
            edited,
            key,
        });
    }
    Ok(out)
}

#[tauri::command]
fn load_clip(ws: Workspace, key: String) -> Result<ClipDetail, String> {
    let root = root_of(&ws.root);
    let clip = read_json(&root.join("clips").join(format!("{key}.json")))
        .ok_or_else(|| format!("no transcript for {key}"))?;
    let (markers, edited) = markers_for(&root, &key);
    let peaks = read_json(&root.join("peaks").join(format!("{key}.json")))
        .unwrap_or(Value::Null);
    let proxy_path = root.join("proxy").join(format!("{key}.mp4"));
    Ok(ClipDetail {
        key,
        clip,
        markers,
        peaks,
        proxy: proxy_path
            .exists()
            .then(|| proxy_path.to_string_lossy().into_owned()),
        edited,
    })
}

#[tauri::command]
fn save_markers(ws: Workspace, key: String, markers: Vec<Value>) -> Result<usize, String> {
    let root = root_of(&ws.root);
    let dir = root.join("markers");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body = serde_json::json!({ "markers": markers });
    fs::write(
        dir.join(format!("{key}.edited.json")),
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(markers.len())
}

#[tauri::command]
fn revert_markers(ws: Workspace, key: String) -> Result<(), String> {
    let path = root_of(&ws.root)
        .join("markers")
        .join(format!("{key}.edited.json"));
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_clips,
            load_clip,
            save_markers,
            revert_markers
        ])
        .run(tauri::generate_context!())
        .expect("error while running chaptr");
}
