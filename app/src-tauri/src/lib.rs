pub mod asr;
pub mod audio;
pub mod beats;
pub mod model;
pub mod scan;
pub mod sidecar;
pub mod tracks;
pub mod workspace;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use asr::AsrConfig;
use model::Library;
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

#[tauri::command]
fn check_sidecars() -> Vec<String> {
    Sidecars::discover().missing()
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
            check_sidecars
        ])
        .run(tauri::generate_context!())
        .expect("error while running chaptr");
}
