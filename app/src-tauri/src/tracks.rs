use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::asr::{self, AsrConfig};
use crate::audio;
use crate::model::{Library, Recording};
use crate::sidecar::Sidecars;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Every source mixed together. Usable, but the worst input for the LLM
    /// because nothing can be attributed.
    Mixed,
    /// Game and system audio. Never transcribed: it yields announcer lines and
    /// hallucination loops, and mixing it in makes the other tracks worse.
    Game,
    /// The person who made the recording.
    Mic,
    /// Everyone else, e.g. a Discord call.
    Voice,
    /// Present in the file, deliberately skipped.
    Ignore,
    /// Detection could not tell. The user must choose.
    Unknown,
}

impl Role {
    pub fn transcribed(self) -> bool {
        matches!(self, Role::Mixed | Role::Mic | Role::Voice | Role::Unknown)
    }

    /// Speaker label attached to every line from a track with this role.
    pub fn speaker(self) -> &'static str {
        match self {
            Role::Mic => "host",
            Role::Voice => "friend",
            _ => "all",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackProbe {
    pub index: usize,
    pub name: String,
    pub words_per_minute: f64,
    /// A little of what was heard, so the user can identify the track by eye.
    pub sample: String,
    pub role: Role,
    /// False when the user needs to confirm, e.g. mic versus voice chat.
    pub confident: bool,
    /// Full probe text. Containment needs all of it; `sample` is truncated for
    /// display and comparing those was why mixed tracks went undetected.
    #[serde(skip)]
    full: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    /// Identifies recordings that share a track arrangement.
    pub signature: String,
    pub track_count: usize,
    pub recordings: usize,
    pub example: String,
    pub tracks: Vec<TrackProbe>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub notes: String,
    /// Roles per layout signature, so one folder can hold recordings made with
    /// different OBS setups.
    #[serde(default)]
    pub roles: BTreeMap<String, Vec<Role>>,
}

impl Settings {
    pub fn roles_for(&self, rec: &Recording) -> Option<&Vec<Role>> {
        self.roles.get(&signature(rec))
    }
}

/// Track names when OBS supplies them, otherwise just the count. Recordings
/// with the same signature are assumed to share a routing setup.
pub fn signature(rec: &Recording) -> String {
    let names: Vec<String> = rec
        .tracks
        .iter()
        .map(|t| t.name.trim().to_lowercase())
        .collect();
    let generic = names
        .iter()
        .all(|n| n.starts_with("track") || n.starts_with("stream") || n.is_empty());
    if generic {
        format!("{}tracks", rec.tracks.len())
    } else {
        names.join("|")
    }
}

/// OBS lets you name tracks, and when someone has, the name is far more
/// reliable than anything inferred from the audio.
fn role_from_name(name: &str) -> Option<Role> {
    let n = name.to_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
    if has(&["mic", "microphone", "aux"]) {
        Some(Role::Mic)
    } else if has(&["discord", "voice", "chat", "comms", "party", "teamspeak"]) {
        Some(Role::Voice)
    } else if has(&["system", "desktop", "game", "application", "media", "main"]) {
        Some(Role::Game)
    } else if has(&["mix", "all", "master", "everything"]) {
        Some(Role::Mixed)
    } else {
        None
    }
}

fn words(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(str::to_string)
        .collect()
}

/// True when `whole` appears to contain everything in `part`, which is how a
/// mixed track is told apart from the individual sources feeding it.
fn contains(whole: &str, part: &str) -> bool {
    let (w, p) = (words(whole), words(part));
    if p.is_empty() {
        return false;
    }
    let shared = p.iter().filter(|x| w.contains(*x)).count();
    shared as f64 / p.len() as f64 > 0.6
}

/// Below this a track is game audio: real conversation scores in the tens or
/// hundreds, game audio scored 0 on every recording tested.
const SPEECH_WPM: f64 = 12.0;

/// Transcribes short samples of every track to work out what each one carries.
/// Runs once per layout rather than once per recording.
pub fn probe_layout(
    sc: &Sidecars,
    cfg: &AsrConfig,
    ws: &Workspace,
    rec: &Recording,
) -> Result<Vec<TrackProbe>> {
    let spots = [0.25, 0.5, 0.75];
    let window = 60.0;
    let mut probes = Vec::new();

    for (i, track) in rec.tracks.iter().enumerate() {
        let mut segments = Vec::new();
        for f in spots {
            let tmp = ws.scratch(&format!("probe{i}"));
            if audio::sample_track(sc, rec, i, rec.duration * f, window, &tmp).is_ok() {
                segments.extend(asr::transcribe(sc, cfg, &tmp, "probe", |_| {})?);
                let _ = std::fs::remove_file(&tmp);
            }
        }
        let wpm = asr::speech_score(&segments, window * spots.len() as f64);
        let full = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        let sample: String = full.chars().take(300).collect();

        probes.push(TrackProbe {
            index: i,
            name: track.name.clone(),
            words_per_minute: wpm,
            sample,
            full,
            role: Role::Unknown,
            confident: false,
        });
    }

    assign(&mut probes);
    Ok(probes)
}

/// Fills in roles from names where possible, then from the audio.
fn assign(probes: &mut [TrackProbe]) {
    for p in probes.iter_mut() {
        if let Some(role) = role_from_name(&p.name) {
            p.role = role;
            p.confident = true;
        }
    }
    if probes.iter().all(|p| p.confident) {
        return;
    }

    for p in probes.iter_mut() {
        if !p.confident && p.words_per_minute < SPEECH_WPM {
            p.role = Role::Game;
            p.confident = true;
        }
    }

    // The mixed track is the one whose speech contains the other tracks'.
    let speech: Vec<usize> = probes
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.confident || p.role.transcribed())
        .filter(|(_, p)| p.words_per_minute >= SPEECH_WPM)
        .map(|(i, _)| i)
        .collect();

    if speech.len() > 1 {
        for &i in &speech {
            let others: Vec<usize> = speech.iter().copied().filter(|&j| j != i).collect();
            let swallows = others
                .iter()
                .all(|&j| contains(&probes[i].full, &probes[j].full));
            if swallows && !probes[i].confident {
                probes[i].role = Role::Mixed;
                probes[i].confident = true;
                break;
            }
        }
    }

    // Whatever is left carries voices, but nothing in the audio says whether it
    // is the recorder's microphone or the rest of the party. Only the user knows.
    for p in probes.iter_mut() {
        if !p.confident {
            p.role = if p.words_per_minute >= SPEECH_WPM { Role::Unknown } else { Role::Ignore };
        }
    }
    if speech.len() == 1 {
        if let Some(p) = probes.iter_mut().find(|p| p.role == Role::Unknown) {
            p.role = Role::Mixed;
            p.confident = true;
        }
    }
}

/// One entry per distinct track arrangement in the library.
pub fn layouts(lib: &Library) -> Vec<(String, &Recording, usize)> {
    let mut seen: BTreeMap<String, (&Recording, usize)> = BTreeMap::new();
    for rec in &lib.recordings {
        let sig = signature(rec);
        let entry = seen.entry(sig).or_insert((rec, 0));
        entry.1 += 1;
        // Probe the longest example: short clips make poor samples.
        if rec.duration > entry.0.duration {
            entry.0 = rec;
        }
    }
    seen.into_iter().map(|(s, (r, n))| (s, r, n)).collect()
}
