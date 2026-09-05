pub mod asr;
pub mod audio;
pub mod chaptrs;
pub mod jobs;
pub mod model;
pub mod project;
pub mod scan;
pub mod sidecar;
pub mod tracks;
pub mod transcribe;
pub mod workspace;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use asr::AsrConfig;
use model::{Chaptr, Library, Transcript};
use project::Project;
use sidecar::Sidecars;
use tracks::{Layout, Role, Settings};

#[derive(Default)]
pub struct App {
    pub library: Mutex<Option<Library>>,
    pub cancel: jobs::Cancel,
    pub running: Mutex<bool>,
}

fn models() -> AsrConfig {
    let dir = model_dir();
    AsrConfig::new(
        dir.join("ggml-large-v3-turbo-q5_0.bin"),
        dir.join("ggml-silero-v5.1.2.bin"),
    )
}

fn model_dir() -> PathBuf {
    std::env::var("CHAPTR_MODELS").map(PathBuf::from).unwrap_or_else(|_| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("models")))
            .unwrap_or_default()
    })
}

#[derive(Serialize)]
pub struct ScanResult {
    library: Library,
    problems: Vec<String>,
}

#[tauri::command]
fn list_projects() -> Vec<Project> {
    project::list()
}

/// Sources can be folders, individual clips, or both.
#[tauri::command]
fn open_project(sources: Vec<String>) -> Result<Project, String> {
    let p = project::open(sources).map_err(|e| e.to_string())?;
    project::adopt_legacy(&p.sources, &p.id);
    Ok(p)
}

#[tauri::command]
fn forget_project(id: String, delete_data: bool) -> Result<(), String> {
    project::forget(&id, delete_data).map_err(|e| e.to_string())
}

#[tauri::command]
fn scan_project(app: State<App>, id: String, gap_minutes: Option<f64>) -> Result<ScanResult, String> {
    let sc = Sidecars::discover();
    sc.require(&sc.ffprobe).map_err(|e| e.to_string())?;
    let p = project::get(&id).ok_or("unknown project")?;

    let (library, problems) = scan::scan(&sc, &p.sources, gap_minutes.unwrap_or(45.0))
        .map_err(|e| e.to_string())?;

    let ws = project::workspace(&id);
    ws.prepare().map_err(|e| e.to_string())?;
    workspace::write_json(&ws.library(), &library).map_err(|e| e.to_string())?;
    project::touch(&id, library.recordings.len(), library.total_duration);

    *app.library.lock().unwrap() = Some(library.clone());
    Ok(ScanResult { library, problems })
}

#[tauri::command]
fn load_library(app: State<App>, id: String) -> Result<Option<Library>, String> {
    let lib: Option<Library> = workspace::maybe_json(&project::workspace(&id).library());
    *app.library.lock().unwrap() = lib.clone();
    Ok(lib)
}

#[tauri::command]
fn get_settings(id: String) -> Settings {
    workspace::maybe_json(&project::workspace(&id).settings()).unwrap_or_default()
}

#[tauri::command]
fn save_settings(id: String, settings: Settings) -> Result<(), String> {
    let ws = project::workspace(&id);
    ws.prepare().map_err(|e| e.to_string())?;
    workspace::write_json(&ws.settings(), &settings).map_err(|e| e.to_string())
}

/// Transcribes samples of every track in each distinct layout and proposes a
/// role for each. Advisory: `unknown` tracks cannot be resolved from audio
/// alone and need the user to say which is which.
#[tauri::command]
fn detect_tracks(id: String) -> Result<Vec<Layout>, String> {
    let sc = Sidecars::discover();
    let ws = project::workspace(&id);
    let lib: Library = workspace::read_json(&ws.library()).map_err(|e| e.to_string())?;
    let cfg = models();
    ws.prepare().map_err(|e| e.to_string())?;

    let mut settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();
    let mut out = Vec::new();

    for (signature, rec, recordings) in tracks::layouts(&lib) {
        let probes = tracks::probe_layout(&sc, &cfg, &ws, rec).map_err(|e| e.to_string())?;
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
fn set_roles(id: String, signature: String, roles: Vec<Role>) -> Result<(), String> {
    let ws = project::workspace(&id);
    let mut settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();
    settings.roles.insert(signature, roles);
    workspace::write_json(&ws.settings(), &settings).map_err(|e| e.to_string())
}

/// User edits live in a separate file, so re-running the beat pass never
/// destroys them.
#[tauri::command]
fn load_chaptrs(id: String) -> Vec<Chaptr> {
    let ws = project::workspace(&id);
    let src = if ws.edits().exists() { ws.edits() } else { ws.chaptrs() };
    workspace::maybe_json::<serde_json::Value>(&src).and_then(|v| {
        // "beats" is the pre-rename key, still present in older projects.
        let list = if v["chaptrs"].is_array() { &v["chaptrs"] } else { &v["beats"] };
        serde_json::from_value(list.clone()).ok()
    })
    .unwrap_or_default()
}

#[tauri::command]
fn save_chaptrs(id: String, chaptrs: Vec<Chaptr>) -> Result<usize, String> {
    let ws = project::workspace(&id);
    ws.prepare().map_err(|e| e.to_string())?;
    workspace::write_json(&ws.edits(), &serde_json::json!({ "chaptrs": chaptrs }))
        .map_err(|e| e.to_string())?;
    Ok(chaptrs.len())
}

#[tauri::command]
fn load_transcript(id: String, recording_id: String) -> Option<Transcript> {
    workspace::maybe_json(&project::workspace(&id).transcript(&recording_id))
}

/// Kicks off a long pass on a worker thread and streams `job` events. Only one
/// runs at a time: both stages want the whole GPU.
#[tauri::command]
fn start_job(
    app_handle: tauri::AppHandle,
    app: State<App>,
    id: String,
    stage: String,
) -> Result<(), String> {
    {
        let mut running = app.running.lock().unwrap();
        if *running {
            return Err("a job is already running".into());
        }
        *running = true;
    }
    app.cancel.reset();

    let cancel = jobs::Cancel(app.cancel.0.clone());
    let asr = models();
    let llm = model_dir().join("Qwen3-14B-Q4_K_M.gguf");

    std::thread::spawn(move || {
        let result = match stage.as_str() {
            "transcribe" => jobs::transcribe_all(&app_handle, &id, &asr, &cancel).map(|_| ()),
            "chaptrs" => jobs::chaptrs_all(&app_handle, &id, llm, &cancel).map(|_| ()),
            other => Err(anyhow::anyhow!("unknown stage {other}")),
        };
        if let Err(e) = result {
            let _ = app_handle.emit(
                "job",
                jobs::Progress {
                    stage,
                    index: 0,
                    total: 0,
                    name: String::new(),
                    fraction: 0.0,
                    done: true,
                    cancelled: false,
                    message: String::new(),
                    error: Some(e.to_string()),
                },
            );
        }
        let state: State<App> = app_handle.state();
        *state.running.lock().unwrap() = false;
    });
    Ok(())
}

#[tauri::command]
fn cancel_job(app: State<App>) {
    app.cancel.request();
}

/// Everything that must exist before a job can run. Models are included
/// because a missing one otherwise surfaces deep into a long pass.
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
    if !model_dir().join("Qwen3-14B-Q4_K_M.gguf").is_file() {
        missing.push("language model".into());
    }
    missing
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(App::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_projects,
            open_project,
            forget_project,
            scan_project,
            load_library,
            get_settings,
            save_settings,
            detect_tracks,
            set_roles,
            load_chaptrs,
            save_chaptrs,
            load_transcript,
            start_job,
            cancel_job,
            check_sidecars
        ])
        .run(tauri::generate_context!())
        .expect("error while running chaptr");
}
