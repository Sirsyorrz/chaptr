use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::model::Segment;
use crate::sidecar::Sidecars;

#[derive(Debug, Clone)]
pub struct AsrConfig {
    pub model: PathBuf,
    pub vad_model: PathBuf,
    pub language: String,
    pub threads: usize,
    pub vad_threshold: f64,
    /// Seeds whisper with expected spellings. Names it has never seen come back
    /// mangled otherwise.
    pub vocabulary: String,
}

impl AsrConfig {
    pub fn new(model: impl Into<PathBuf>, vad_model: impl Into<PathBuf>) -> Self {
        Self {
            model: model.into(),
            vad_model: vad_model.into(),
            language: "en".into(),
            threads: std::thread::available_parallelism().map_or(8, |n| n.get().min(16)),
            vad_threshold: 0.5,
            vocabulary: String::new(),
        }
    }
}

/// Transcribes one 16 kHz mono WAV.
///
/// VAD is not optional. Without it whisper fills silence with repetition loops
/// — the same sentence emitted seven times — and on game footage there is a lot
/// of silence. It also makes the pass roughly twice as fast, because skipped
/// silence is never decoded.
pub fn transcribe(
    sc: &Sidecars,
    cfg: &AsrConfig,
    wav: &Path,
    who: &str,
    mut on_progress: impl FnMut(f64),
) -> Result<Vec<Segment>> {
    sc.require(&sc.whisper)?;
    if !cfg.model.is_file() {
        bail!("whisper model missing: {}", cfg.model.display());
    }

    // Not with_extension(""): filenames carry a ".t0" track marker that Path
    // would mistake for the extension and strip.
    let stem = PathBuf::from(wav.to_string_lossy().trim_end_matches(".wav").to_string());
    let json_path = PathBuf::from(format!("{}.json", stem.to_string_lossy()));

    let mut cmd = Command::new(&sc.whisper);
    cmd.arg("-m").arg(&cfg.model)
        .arg("-f").arg(wav)
        .args(["-l", &cfg.language])
        .args(["-t", &cfg.threads.to_string()])
        .arg("--vad")
        .arg("-vm").arg(&cfg.vad_model)
        .args(["-vt", &cfg.vad_threshold.to_string()])
        // Stops the model narrating music stings and gunfire as dialogue.
        .arg("-sns")
        // Results come from the JSON file, so the running transcript on stdout
        // is dead weight. Printing it and not draining the pipe deadlocks the
        // child once 64 KB have accumulated, which a long recording always hits.
        .arg("-np")
        .arg("-pp")
        .arg("-oj")
        .arg("-of").arg(&stem)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    if !cfg.vocabulary.trim().is_empty() {
        cmd.arg("--prompt").arg(cfg.vocabulary.trim());
    }

    let mut child = cmd.spawn().context("spawning whisper-cli")?;

    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(pct) = line.rsplit_once("progress = ") {
                if let Ok(v) = pct.1.trim().trim_end_matches('%').parse::<f64>() {
                    on_progress((v / 100.0).clamp(0.0, 1.0));
                }
            }
        }
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("whisper failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }

    let data: Value = serde_json::from_str(&std::fs::read_to_string(&json_path)?)
        .with_context(|| format!("parsing {}", json_path.display()))?;
    let _ = std::fs::remove_file(&json_path);

    let empty = vec![];
    let segments = data["transcription"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|s| {
            let text = s["text"].as_str()?.trim().to_string();
            if text.is_empty() {
                return None;
            }
            Some(Segment {
                start: s["offsets"]["from"].as_f64()? / 1000.0,
                end: s["offsets"]["to"].as_f64()? / 1000.0,
                text,
                who: who.to_string(),
            })
        })
        .collect();

    on_progress(1.0);
    Ok(segments)
}

/// Whisper emits these when handed audio with no speech in it.
const NOISE: [&str; 6] = [
    "[blank_audio]", "[silence]", "(silence)", "[music]", "(music)", "[sound]",
];

fn is_noise(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    NOISE.iter().any(|n| t == *n) || t.chars().all(|c| !c.is_alphanumeric())
}

/// How much real conversation a track carries, in words per minute.
///
/// Game-audio tracks score near zero: they yield a couple of announcer lines
/// and then degenerate strings like "I I I I". Voice tracks score in the
/// hundreds. The gap is wide enough that the exact threshold barely matters.
pub fn speech_score(segments: &[Segment], sampled_secs: f64) -> f64 {
    if sampled_secs <= 0.0 {
        return 0.0;
    }
    let words: usize = segments
        .iter()
        .filter(|s| !is_noise(&s.text))
        .map(|s| {
            let ws: Vec<&str> = s.text.split_whitespace().collect();
            // A run of one repeated token is a hallucination, not speech.
            let unique: std::collections::HashSet<_> =
                ws.iter().map(|w| w.to_lowercase()).collect();
            if ws.len() > 4 && unique.len() <= 2 {
                0
            } else {
                ws.len()
            }
        })
        .sum();
    words as f64 / (sampled_secs / 60.0)
}
