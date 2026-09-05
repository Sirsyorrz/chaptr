use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Result};

use crate::asr::{self, AsrConfig};
use crate::audio;
use crate::model::{Recording, Segment, Transcript};
use crate::sidecar::Sidecars;
use crate::tracks::Role;
use crate::workspace::Workspace;

/// How a recording's tracks are turned into one labelled transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// Separate microphone and voice-chat tracks. Attribution is exact.
    Split { mic: usize, voice: usize },
    /// A mixed track plus the microphone. Everyone is audible in the mix, and
    /// the mic says which of those lines were the host's.
    Derived { mixed: usize, mic: usize },
    /// One usable track and no way to tell speakers apart.
    Single { track: usize, speaker: &'static str },
}

pub fn plan(roles: &[Role]) -> Result<Plan> {
    let find = |want: Role| roles.iter().position(|r| *r == want);
    let mic = find(Role::Mic);
    let voice = find(Role::Voice);
    let mixed = find(Role::Mixed);

    Ok(match (mic, voice, mixed) {
        (Some(mic), Some(voice), _) => Plan::Split { mic, voice },
        (Some(mic), None, Some(mixed)) => Plan::Derived { mixed, mic },
        (None, Some(voice), Some(mixed)) => Plan::Derived { mixed, mic: voice },
        (_, _, Some(mixed)) => Plan::Single { track: mixed, speaker: "all" },
        (Some(mic), None, None) => Plan::Single { track: mic, speaker: "host" },
        (None, Some(voice), None) => Plan::Single { track: voice, speaker: "friend" },
        _ => match roles.iter().position(|r| *r == Role::Unknown) {
            Some(track) => Plan::Single { track, speaker: "all" },
            None => bail!("no transcribable audio track; check the track roles"),
        },
    })
}

fn words(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether a line from the mixed track also appears on the microphone, and so
/// was spoken by the host rather than by someone else in the call.
///
/// Deliberately loose. The two tracks are transcribed independently, so the
/// same speech comes back with different wording and slightly different
/// timings; requiring an exact match would label almost nothing.
fn same_line(a: &Segment, b: &Segment) -> bool {
    if (a.start - b.start).abs() > 3.0 {
        return false;
    }
    let (x, y) = (words(&a.text), words(&b.text));
    let smaller = x.len().min(y.len());
    if smaller == 0 {
        return false;
    }
    let shared = x.iter().filter(|w| y.contains(*w)).count();
    shared as f64 / smaller as f64 > 0.6
}

fn label_against(mixed: &mut [Segment], mic: &[Segment]) {
    for seg in mixed.iter_mut() {
        // Mic segments are ordered, so only the ones near this line matter.
        let nearby = mic.iter().filter(|m| (m.start - seg.start).abs() <= 3.0);
        seg.who = if nearby.clone().any(|m| same_line(seg, m)) { "host" } else { "friend" }.into();
    }
}

fn asr_track(
    sc: &Sidecars,
    cfg: &AsrConfig,
    ws: &Workspace,
    rec: &Recording,
    track: usize,
    speaker: &str,
    on_progress: impl FnMut(f64),
) -> Result<Vec<Segment>> {
    let wav = ws.scratch(&format!("{}.t{track}", rec.id));
    audio::sample_track(sc, rec, track, 0.0, rec.duration, &wav)?;
    let segments = asr::transcribe(sc, cfg, &wav, speaker, on_progress)?;
    let _ = std::fs::remove_file(&wav);
    Ok(segments)
}

/// Produces one transcript for a recording, with speaker labels wherever the
/// track layout makes them possible.
pub fn run(
    sc: &Sidecars,
    cfg: &AsrConfig,
    ws: &Workspace,
    rec: &Recording,
    roles: &[Role],
    dest: &Path,
    mut on_progress: impl FnMut(f64),
) -> Result<Transcript> {
    let plan = plan(roles)?;

    let mut segments = match plan {
        Plan::Split { mic, voice } => {
            let mut a = asr_track(sc, cfg, ws, rec, mic, "host", |p| on_progress(p * 0.5))?;
            let b = asr_track(sc, cfg, ws, rec, voice, "friend", |p| {
                on_progress(0.5 + p * 0.5)
            })?;
            a.extend(b);
            a
        }
        Plan::Derived { mixed, mic } => {
            let mut all = asr_track(sc, cfg, ws, rec, mixed, "all", |p| on_progress(p * 0.5))?;
            let host = asr_track(sc, cfg, ws, rec, mic, "host", |p| on_progress(0.5 + p * 0.5))?;
            label_against(&mut all, &host);
            all
        }
        Plan::Single { track, speaker } => {
            asr_track(sc, cfg, ws, rec, track, speaker, &mut on_progress)?
        }
    };

    segments.sort_by(|a, b| a.start.total_cmp(&b.start));

    let transcript = Transcript {
        recording_id: rec.id.clone(),
        duration: rec.duration,
        model: cfg.model.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        segments,
    };
    crate::workspace::write_json(dest, &transcript)?;
    Ok(transcript)
}
