use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::model::Recording;
use crate::sidecar::Sidecars;

/// whisper.cpp only accepts 16 kHz mono PCM.
pub const ASR_RATE: u32 = 16_000;

fn run_ffmpeg(
    sc: &Sidecars,
    args: Vec<String>,
    duration: f64,
    mut on_progress: impl FnMut(f64),
) -> Result<()> {
    let mut child = Command::new(&sc.ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-progress", "pipe:1", "-y"])
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning ffmpeg")?;

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(us) = line.strip_prefix("out_time_us=") {
                if let Ok(us) = us.trim().parse::<f64>() {
                    if duration > 0.0 {
                        on_progress((us / 1e6 / duration).clamp(0.0, 1.0));
                    }
                }
            }
        }
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    on_progress(1.0);
    Ok(())
}

/// One 16 kHz mono WAV per audio track, in a single decode pass.
///
/// Deliberately *not* an `amix` filtergraph. Feeding several streams of one
/// input into a filter drops frames without erroring: on a 2h44m recording it
/// silently produced 11-13 minutes of audio, and a different length each run.
/// Plain per-stream `-map` outputs are both correct and faster.
pub fn extract_tracks(
    sc: &Sidecars,
    rec: &Recording,
    dir: &Path,
    on_progress: impl FnMut(f64),
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut args = vec!["-i".to_string(), rec.path.clone(), "-vn".to_string()];
    let mut outs = Vec::new();

    for (i, _) in rec.tracks.iter().enumerate() {
        let out = dir.join(format!("{}.t{i}.wav", rec.id));
        args.extend([
            "-map".into(),
            format!("0:a:{i}"),
            "-ac".into(),
            "1".into(),
            "-ar".into(),
            ASR_RATE.to_string(),
            "-c:a".into(),
            "pcm_s16le".into(),
            out.to_string_lossy().into_owned(),
        ]);
        outs.push(out);
    }

    run_ffmpeg(sc, args, rec.duration, on_progress)?;
    Ok(outs)
}

/// A short sample from one track, used to work out what the track contains.
pub fn sample_track(
    sc: &Sidecars,
    rec: &Recording,
    track: usize,
    at: f64,
    secs: f64,
    dest: &Path,
) -> Result<()> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let args = vec![
        "-ss".into(),
        at.to_string(),
        "-t".into(),
        secs.to_string(),
        "-i".into(),
        rec.path.clone(),
        "-vn".into(),
        "-map".into(),
        format!("0:a:{track}"),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        ASR_RATE.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        dest.to_string_lossy().into_owned(),
    ];
    run_ffmpeg(sc, args, secs, |_| {})
}

/// Small Opus copy of the speech track, kept for previewing a beat.
/// Roughly 10 MB per hour, so a 100 hour job costs about a gigabyte.
pub fn extract_preview(
    sc: &Sidecars,
    rec: &Recording,
    track: usize,
    dest: &Path,
    on_progress: impl FnMut(f64),
) -> Result<()> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let args = vec![
        "-i".into(),
        rec.path.clone(),
        "-vn".into(),
        "-map".into(),
        format!("0:a:{track}"),
        "-ac".into(),
        "1".into(),
        "-c:a".into(),
        "libopus".into(),
        "-b:a".into(),
        "24k".into(),
        "-application".into(),
        "voip".into(),
        dest.to_string_lossy().into_owned(),
    ];
    run_ffmpeg(sc, args, rec.duration, on_progress)
}
