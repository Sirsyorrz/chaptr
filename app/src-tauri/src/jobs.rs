use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::asr::AsrConfig;
use crate::chaptrs::{self, ChaptrConfig, Llm};
use crate::model::{Chaptr, Library, Transcript};
use crate::project;
use crate::sidecar::Sidecars;
use crate::tracks::Settings;
use crate::transcribe;
use crate::workspace;

/// Emitted on the `job` channel as work proceeds. The UI needs to show which
/// file of how many, because a hundred-hour pass runs for hours.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// Increments per run. Without it the UI cannot tell a fresh update from
    /// the leftover `done` of the previous job and stops watching immediately.
    pub run: u64,
    pub stage: String,
    pub index: usize,
    pub total: usize,
    pub name: String,
    /// Progress within the current file, 0 to 1.
    pub fraction: f64,
    pub done: bool,
    pub cancelled: bool,
    pub message: String,
    pub error: Option<String>,
}

impl Progress {
    pub fn new(stage: &str) -> Self {
        Self {
            run: 0,
            stage: stage.into(),
            index: 0,
            total: 0,
            name: String::new(),
            fraction: 0.0,
            done: false,
            cancelled: false,
            message: String::new(),
            error: None,
        }
    }
}

#[derive(Default)]
pub struct Cancel(pub Arc<AtomicBool>);

impl Cancel {
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
    pub fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn hit(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Pushed as an event *and* stored, so the UI can poll. Events alone are one
/// silent failure away from a job that runs invisibly for an hour.
fn emit(app: &AppHandle, p: &Progress) {
    use tauri::Manager;
    let mut p = p.clone();
    if let Some(state) = app.try_state::<crate::App>() {
        p.run = *state.run.lock().unwrap();
        *state.last_job.lock().unwrap() = Some(p.clone());
    }
    let _ = app.emit("job", &p);
}

/// Transcribes every recording that does not already have a transcript, so an
/// interrupted run resumes instead of starting over.
pub fn transcribe_all(
    app: &AppHandle,
    id: &str,
    cfg: &AsrConfig,
    cancel: &Cancel,
) -> Result<usize> {
    let sc = Sidecars::discover();
    let ws = project::workspace(id);
    let lib: Library = workspace::read_json(&ws.library())?;
    let settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();
    if settings.roles.is_empty() {
        bail!("no track roles yet — open Tracks and detect them first");
    }
    ws.prepare()?;

    let todo: Vec<_> = lib
        .recordings
        .iter()
        .filter(|r| !ws.transcript(&r.id).exists())
        .collect();

    let mut p = Progress::new("transcribe");
    p.total = todo.len();
    if todo.is_empty() {
        p.done = true;
        p.message = "every recording is already transcribed".into();
        emit(app, &p);
        return Ok(0);
    }

    let mut count = 0;
    for (i, rec) in todo.iter().enumerate() {
        if cancel.hit() {
            p.cancelled = true;
            p.done = true;
            emit(app, &p);
            return Ok(count);
        }
        p.index = i + 1;
        p.name = rec.name.clone();
        p.fraction = 0.0;
        emit(app, &p);

        let Some(roles) = settings.roles_for(rec) else {
            continue;
        };
        let dest = ws.transcript(&rec.id);
        let result = transcribe::run(&sc, cfg, &ws, rec, roles, &dest, |f| {
            let mut tick = p.clone();
            tick.fraction = f;
            emit(app, &tick);
        });

        match result {
            Ok(_) => count += 1,
            Err(e) => {
                p.error = Some(format!("{}: {e}", rec.name));
                emit(app, &p);
                p.error = None;
            }
        }
    }

    ws.clear_scratch();
    project::mark_unsaved(id);
    p.done = true;
    p.message = format!("transcribed {count} recordings");
    emit(app, &p);
    Ok(count)
}

/// Runs the language model over every transcript and writes chaptrs.json.
pub fn chaptrs_all(
    app: &AppHandle,
    id: &str,
    model: std::path::PathBuf,
    cancel: &Cancel,
) -> Result<usize> {
    let ws = project::workspace(id);
    let lib: Library = workspace::read_json(&ws.library())?;
    let settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();

    let todo: Vec<_> = lib
        .recordings
        .iter()
        .filter(|r| ws.transcript(&r.id).exists())
        .collect();

    let mut p = Progress::new("chaptrs");
    p.total = todo.len();
    if todo.is_empty() {
        bail!("nothing transcribed yet — run Transcribe first");
    }

    let mut cfg = ChaptrConfig::new(model);
    if !settings.subject.trim().is_empty() {
        cfg.subject = settings.subject.clone();
    }
    cfg.notes = settings.notes.clone();
    cfg.window_secs = settings.window_minutes.max(0.5) * 60.0;
    cfg.overlap_secs = settings.overlap_minutes.clamp(0.0, settings.window_minutes / 2.0) * 60.0;
    cfg.min_per_window = settings.min_per_window;
    cfg.max_per_window = settings.max_per_window.max(settings.min_per_window);
    cfg.temperature = settings.temperature;
    cfg.remote = crate::chaptrs::Remote::from(&crate::prefs::load());

    p.message = if cfg.remote.is_some() {
        "contacting the model".into()
    } else {
        "loading the language model".into()
    };
    emit(app, &p);
    let llm = Llm::start(&Sidecars::discover(), cfg)?;
    p.message.clear();

    let mut all: Vec<Chaptr> = Vec::new();
    for (i, rec) in todo.iter().enumerate() {
        if cancel.hit() {
            p.cancelled = true;
            p.done = true;
            emit(app, &p);
            return Ok(all.len());
        }
        p.index = i + 1;
        p.name = rec.name.clone();
        p.fraction = 0.0;
        emit(app, &p);

        let tr: Transcript = workspace::read_json(&ws.transcript(&rec.id))?;
        match chaptrs::run(&llm, rec, &tr.segments, |f| {
            let mut tick = p.clone();
            tick.fraction = f;
            emit(app, &tick);
        }) {
            Ok(got) => all.extend(got),
            Err(e) => {
                p.error = Some(format!("{}: {e}", rec.name));
                emit(app, &p);
                p.error = None;
            }
        }
    }

    all.sort_by(|a, b| a.global.total_cmp(&b.global));
    workspace::write_json(
        &ws.root.join("chaptrs.json"),
        &serde_json::json!({ "chaptrs": all }),
    )?;

    project::mark_unsaved(id);
    p.done = true;
    p.message = format!("{} chaptrs", all.len());
    emit(app, &p);
    Ok(all.len())
}
