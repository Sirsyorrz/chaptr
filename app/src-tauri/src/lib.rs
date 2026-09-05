pub mod asr;
pub mod audio;
pub mod beats;
pub mod model;
pub mod scan;
pub mod sidecar;
pub mod tracks;
pub mod transcribe;
pub mod workspace;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use asr::AsrConfig;
use model::{Beat, Library, Transcript};
use sidecar::Sidecars;
use tracks::{Layout, Role, Settings};
use workspace::Workspace;

#[derive(Default)]
pub struct App {
    pub library: Mutex<Option<Library>>,
}

fn ws_for(footage: &str) -> Workspace {
    Workspace::new(PathBuf::from(footage).join(".chaptr"))
}

fn models() -> AsrConfig {
    let dir = std::env::var("CHAPTR_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("models")))
                .unwrap_or_default()
        });
    AsrConfig::new(
        dir.join("ggml-large-v3-turbo-q5_0.bin"),
        dir.join("ggml-silero-v5.1.2.bin"),
    )
}

#[derive(Serialize)]
pub struct ScanResult {
    library: Library,
    problems: Vec<String>,
}

#[tauri::command]
fn scan_folder(
    app: State<App>,
    folder: String,
    gap_minutes: Option<f64>,
) -> Result<ScanResult, String> {
    let sc = Sidecars::discover();
    sc.require(&sc.ffprobe).map_err(|e| e.to_string())?;

    let (library, problems) = scan::scan(&sc, &PathBuf::from(&folder), gap_minutes.unwrap_or(45.0))
        .map_err(|e| e.to_string())?;

    let ws = ws_for(&folder);
    ws.prepare().map_err(|e| e.to_string())?;
    workspace::write_json(&ws.library(), &library).map_err(|e| e.to_string())?;

    *app.library.lock().unwrap() = Some(library.clone());
    Ok(ScanResult { library, problems })
}

#[tauri::command]
fn load_library(app: State<App>, folder: String) -> Result<Option<Library>, String> {
    let lib: Option<Library> = workspace::maybe_json(&ws_for(&folder).library());
    *app.library.lock().unwrap() = lib.clone();
    Ok(lib)
}

#[tauri::command]
fn get_settings(folder: String) -> Settings {
    workspace::maybe_json(&ws_for(&folder).settings()).unwrap_or_default()
}

#[tauri::command]
fn save_settings(folder: String, settings: Settings) -> Result<(), String> {
    let ws = ws_for(&folder);
    ws.prepare().map_err(|e| e.to_string())?;
    workspace::write_json(&ws.settings(), &settings).map_err(|e| e.to_string())
}

/// Transcribes samples of every track in each distinct layout and proposes a
/// role for each. The proposal is advisory: tracks marked `unknown` cannot be
/// resolved from audio alone and need the user to say which is which.
#[tauri::command]
fn detect_tracks(folder: String) -> Result<Vec<Layout>, String> {
    let sc = Sidecars::discover();
    let ws = ws_for(&folder);
    let lib: Library = workspace::read_json(&ws.library()).map_err(|e| e.to_string())?;
    let cfg = models();
    ws.prepare().map_err(|e| e.to_string())?;

    let mut settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();
    let mut out = Vec::new();

    for (signature, rec, recordings) in tracks::layouts(&lib) {
        let probes =
            tracks::probe_layout(&sc, &cfg, &ws, rec).map_err(|e| e.to_string())?;
        // Never clobber roles the user has already confirmed.
        settings
            .roles
            .entry(signature.clone())
            .or_insert_with(|| probes.iter().map(|p| p.role).collect());
        out.push(Layout {
            track_count: probes.len(),
            example: rec.name.clone(),
            tracks: probes,
            signature,
            recordings,
        });
    }

    workspace::write_json(&ws.settings(), &settings).map_err(|e| e.to_string())?;
    ws.clear_scratch();
    Ok(out)
}

#[tauri::command]
fn set_roles(folder: String, signature: String, roles: Vec<Role>) -> Result<(), String> {
    let ws = ws_for(&folder);
    let mut settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();
    settings.roles.insert(signature, roles);
    workspace::write_json(&ws.settings(), &settings).map_err(|e| e.to_string())
}

/// User edits live in a separate file, so re-running the beat pass never
/// destroys them.
#[tauri::command]
fn load_beats(folder: String) -> Vec<Beat> {
    let ws = ws_for(&folder);
    let src = if ws.edits().exists() { ws.edits() } else { ws.beats() };
    workspace::maybe_json::<serde_json::Value>(&src)
        .and_then(|v| serde_json::from_value(v["beats"].clone()).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn save_beats(folder: String, beats: Vec<Beat>) -> Result<usize, String> {
    let ws = ws_for(&folder);
    ws.prepare().map_err(|e| e.to_string())?;
    workspace::write_json(&ws.edits(), &serde_json::json!({ "beats": beats }))
        .map_err(|e| e.to_string())?;
    Ok(beats.len())
}

#[tauri::command]
fn load_transcript(folder: String, recording_id: String) -> Option<Transcript> {
    workspace::maybe_json(&ws_for(&folder).transcript(&recording_id))
}

/// Everything that must exist before a job can run. Models are included
/// because a missing one otherwise surfaces as a failure deep into a long pass.
#[tauri::command]
fn check_sidecars() -> Vec<String> {
    let mut missing = Sidecars::discover().missing();
    let cfg = models();
    for (label, path) in [
        ("speech model", &cfg.model),
        ("voice-activity model", &cfg.vad_model),
    ] {
        if !path.is_file() {
            missing.push(label.to_string());
        }
    }
    if !beats::BeatConfig::new(llm_model()).model.is_file() {
        missing.push("language model".into());
    }
    missing
}

fn llm_model() -> PathBuf {
    std::env::var("CHAPTR_LLM").map(PathBuf::from).unwrap_or_else(|_| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("models").join("Qwen3-14B-Q4_K_M.gguf")))
            .unwrap_or_default()
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(App::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            load_library,
            get_settings,
            save_settings,
            detect_tracks,
            set_roles,
            load_beats,
            save_beats,
            load_transcript,
            check_sidecars
        ])
        .run(tauri::generate_context!())
        .expect("error while running chaptr");
}
