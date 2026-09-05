pub mod asr;
pub mod audio;
pub mod beats;
pub mod model;
pub mod scan;
pub mod sidecar;
pub mod workspace;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use model::Library;
use sidecar::Sidecars;
use workspace::Workspace;

#[derive(Default)]
pub struct App {
    pub library: Mutex<Option<Library>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub footage: String,
    pub workspace: String,
    pub gap_minutes: f64,
}

fn ws_for(footage: &str) -> Workspace {
    Workspace::new(PathBuf::from(footage).join(".chaptr"))
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

    let root = PathBuf::from(&folder);
    let (library, problems) =
        scan::scan(&sc, &root, gap_minutes.unwrap_or(45.0)).map_err(|e| e.to_string())?;

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
            check_sidecars
        ])
        .run(tauri::generate_context!())
        .expect("error while running chaptr");
}
