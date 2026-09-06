pub mod asr;
pub mod audio;
pub mod bundle;
pub mod catalogue;
pub mod chaptrs;
pub mod download;
pub mod jobs;
pub mod model;
pub mod prefs;
pub mod resolve;
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
    pub last_job: Mutex<Option<jobs::Progress>>,
    pub downloads: download::Downloads,
    pub run: Mutex<u64>,
}

fn models() -> AsrConfig {
    let dir = model_dir();
    let p = prefs::load();
    let mut cfg = AsrConfig::new(dir.join(&p.whisper_model), dir.join("ggml-silero-v5.1.2.bin"));
    cfg.language = p.language.clone();
    cfg.vad_threshold = p.vad_threshold;
    cfg
}

/// Transcription config for a project, adding its vocabulary hint.
fn asr_for(id: &str) -> AsrConfig {
    let mut cfg = models();
    let ws = project::workspace(id);
    if let Some(s) = workspace::maybe_json::<Settings>(&ws.settings()) {
        cfg.vocabulary = s.notes.clone();
    }
    cfg
}

#[tauri::command]
fn get_prefs() -> prefs::Prefs {
    prefs::load()
}

#[tauri::command]
fn save_prefs(prefs_in: prefs::Prefs) -> Result<(), String> {
    prefs::save(&prefs_in).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct CatalogueEntry {
    #[serde(flatten)]
    entry: catalogue::Entry,
    installed: bool,
    on_disk: u64,
}

#[tauri::command]
fn list_catalogue() -> Vec<CatalogueEntry> {
    let dir = model_dir();
    catalogue::ENTRIES
        .iter()
        .map(|e| {
            let path = dir.join(e.file);
            let on_disk = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            CatalogueEntry { entry: e.clone(), installed: on_disk > 0, on_disk }
        })
        .collect()
}

#[tauri::command]
fn gpu_vram() -> Option<u32> {
    catalogue::vram_gb()
}

#[tauri::command]
fn download_model(app_handle: tauri::AppHandle, app: State<App>, id: String) -> Result<(), String> {
    let entry = catalogue::find(&id).ok_or("unknown model")?;
    let dest = model_dir().join(entry.file);
    let cancel = app.downloads.cancel.clone();
    cancel.store(false, std::sync::atomic::Ordering::SeqCst);
    let url = entry.url.to_string();

    let file = entry.file.to_string();
    std::thread::spawn(move || {
        let report = |received, total, done, error| {
            let _ = app_handle.emit(
                "download",
                download::DownloadProgress {
                    id: id.clone(),
                    file: file.clone(),
                    received,
                    total,
                    done,
                    error,
                },
            );
        };
        let result = download::fetch(&url, &dest, &cancel, |received, total| {
            report(received, total, false, None)
        });
        match result {
            Ok(n) => report(n, n, true, None),
            Err(e) => report(0, 0, true, Some(e.to_string())),
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_download(app: State<App>) {
    app.downloads.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
fn delete_model(id: String) -> Result<(), String> {
    let entry = catalogue::find(&id).ok_or("unknown model")?;
    std::fs::remove_file(model_dir().join(entry.file)).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_models() -> prefs::Models {
    prefs::available(&model_dir())
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

#[derive(Serialize)]
struct ResolveLink {
    installed: bool,
    scripts_dir: String,
}

#[tauri::command]
fn resolve_link() -> ResolveLink {
    ResolveLink {
        installed: resolve::installed(),
        scripts_dir: resolve::scripts_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

#[tauri::command]
fn install_resolve_link() -> Result<String, String> {
    resolve::install()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

/// Ask Resolve to move its playhead. Cheap and silent: if nothing is watching,
/// this just leaves a file behind.
#[tauri::command]
fn resolve_goto(id: String, recording: String, offset: f64) -> Result<(), String> {
    let ws = project::workspace(&id);
    let library: Library = workspace::maybe_json(&ws.library()).ok_or("no library")?;
    let rec = library
        .recordings
        .iter()
        .find(|r| r.id == recording)
        .ok_or("unknown recording")?;
    resolve::goto(&rec.path, offset).map_err(|e| e.to_string())
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
    project::mark_unsaved(&id);

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

/// The track layouts in this project, without listening to anything. Lets the
/// roles be set by hand, which is usually all that is needed: a recording setup
/// rarely changes between sessions.
#[tauri::command]
fn list_layouts(id: String) -> Vec<Layout> {
    let ws = project::workspace(&id);
    let Some(lib) = workspace::maybe_json::<Library>(&ws.library()) else {
        return Vec::new();
    };
    let settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();

    tracks::layouts(&lib)
        .into_iter()
        .map(|(signature, rec, recordings)| {
            let saved = settings.roles.get(&signature);
            Layout {
                track_count: rec.tracks.len(),
                recordings,
                example: rec.name.clone(),
                tracks: rec
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        tracks::TrackProbe::unheard(
                            i,
                            &t.name,
                            t.channels,
                            saved.and_then(|r| r.get(i).copied()).unwrap_or(Role::Unknown),
                            saved.is_some(),
                        )
                    })
                    .collect(),
                signature,
            }
        })
        .collect()
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
    workspace::write_json(&ws.root.join("chaptrs.edits.json"), &serde_json::json!({ "chaptrs": chaptrs }))
        .map_err(|e| e.to_string())?;
    project::mark_unsaved(&id);
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
    // Drop the previous run's final state, or the first poll of this one reads
    // a stale `done` and the UI stops watching before any work happens.
    *app.run.lock().unwrap() += 1;
    *app.last_job.lock().unwrap() = None;

    let cancel = jobs::Cancel(app.cancel.0.clone());
    let asr = asr_for(&id);
    let p = prefs::load();
    let llm = model_dir().join(&p.local_model);

    std::thread::spawn(move || {
        let result = match stage.as_str() {
            "transcribe" => jobs::transcribe_all(&app_handle, &id, &asr, &cancel).map(|_| ()),
            "chaptrs" => jobs::chaptrs_all(&app_handle, &id, llm, &cancel).map(|_| ()),
            other => Err(anyhow::anyhow!("unknown stage {other}")),
        };
        let state: State<App> = app_handle.state();
        if let Err(e) = result {
            let mut p = jobs::Progress::new(&stage);
            p.run = *state.run.lock().unwrap();
            p.done = true;
            p.error = Some(e.to_string());
            *state.last_job.lock().unwrap() = Some(p.clone());
            let _ = app_handle.emit("job", &p);
        }
        *state.running.lock().unwrap() = false;
    });
    Ok(())
}

#[tauri::command]
fn cancel_job(app: State<App>) {
    app.cancel.request();
}

#[tauri::command]
fn job_status(app: State<App>) -> Option<jobs::Progress> {
    app.last_job.lock().unwrap().clone()
}

/// Per-recording state, so the user can see what has actually been produced.
#[derive(Serialize)]
pub struct FileStatus {
    id: String,
    name: String,
    duration: f64,
    global_offset: f64,
    session_id: usize,
    segments: usize,
    chaptrs: usize,
    transcribed: bool,
}

#[tauri::command]
fn file_status(id: String) -> Vec<FileStatus> {
    let ws = project::workspace(&id);
    let Some(lib) = workspace::maybe_json::<Library>(&ws.library()) else {
        return Vec::new();
    };
    let chaptrs = load_chaptrs(id.clone());

    lib.recordings
        .iter()
        .map(|r| {
            let tr: Option<Transcript> = workspace::maybe_json(&ws.transcript(&r.id));
            FileStatus {
                segments: tr.as_ref().map_or(0, |t| t.segments.len()),
                transcribed: tr.is_some(),
                chaptrs: chaptrs.iter().filter(|c| c.recording_id == r.id).count(),
                id: r.id.clone(),
                name: r.name.clone(),
                duration: r.duration,
                global_offset: r.global_offset,
                session_id: r.session_id,
            }
        })
        .collect()
}

/// Everything that must exist before a job can run. Models are included
/// because a missing one otherwise surfaces deep into a long pass.
/// Writes the whole project to a single file the user chooses. Everything up
/// to now lived in a working cache; this is the copy they own.
#[tauri::command]
fn save_project(id: String, path: Option<String>) -> Result<String, String> {
    let p = project::get(&id).ok_or("unknown project")?;
    let dest = match path.or(p.file.clone()) {
        Some(d) => PathBuf::from(d),
        None => return Err("no file chosen yet".into()),
    };
    let dest = if dest.extension().is_some() {
        dest
    } else {
        dest.with_extension(bundle::EXTENSION)
    };

    let ws = project::workspace(&id);
    bundle::save(&ws, &p.name, &p.sources, &dest).map_err(|e| e.to_string())?;

    let shown = dest.to_string_lossy().into_owned();
    let name = dest
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.name.clone());
    project::update(&id, |p| {
        p.file = Some(shown.clone());
        p.name = name;
        p.unsaved = false;
    });
    Ok(shown)
}

/// Opens a saved .chaptr file, restoring its working directory.
#[tauri::command]
fn open_project_file(path: String) -> Result<Project, String> {
    let file = PathBuf::from(&path);
    let b = bundle::read(&file).map_err(|e| e.to_string())?;

    let mut p = project::open(b.sources.clone()).map_err(|e| e.to_string())?;
    let ws = project::workspace(&p.id);
    bundle::unpack(&b, &ws).map_err(|e| e.to_string())?;

    p.file = Some(path);
    p.name = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(p.name);
    p.unsaved = false;
    if let Some(lib) = &b.library {
        p.recordings = lib.recordings.len();
        p.duration = lib.total_duration;
    }
    project::put_project(p).map_err(|e| e.to_string())
}

/// Closes the current project and clears its working cache. One project is
/// open at a time, so nothing is left behind for a project nobody is editing.
#[tauri::command]
fn close_project(app: State<App>, id: String, discard: bool) -> Result<(), String> {
    *app.library.lock().unwrap() = None;
    if discard {
        project::forget(&id, true).map_err(|e| e.to_string())?;
    }
    Ok(())
}

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
    let p = prefs::load();
    if p.engine == prefs::Engine::Local && !model_dir().join(&p.local_model).is_file() {
        missing.push("language model".into());
    }
    if p.engine == prefs::Engine::Cloud && p.key().is_empty() {
        missing.push("API key".into());
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
            resolve_link,
            install_resolve_link,
            resolve_goto,
            open_project,
            forget_project,
            scan_project,
            load_library,
            get_settings,
            save_settings,
            list_layouts,
            detect_tracks,
            set_roles,
            load_chaptrs,
            save_chaptrs,
            load_transcript,
            start_job,
            cancel_job,
            job_status,
            file_status,
            save_project,
            close_project,
            open_project_file,
            get_prefs,
            save_prefs,
            list_models,
            list_catalogue,
            gpu_vram,
            download_model,
            cancel_download,
            delete_model,
            check_sidecars
        ])
        .run(tauri::generate_context!())
        .expect("error while running chaptr");
}
