use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::model::Recording;
use crate::sidecar::Sidecars;

/// whisper.cpp only accepts 16 kHz mono PCM.
const ASR_RATE: u32 = 16_000;

/// Recordings often carry several audio tracks (game, mic, voice chat). Without
/// knowing which is which, mixing them all is the only choice that never
/// silently discards someone's voice.
fn mix_args(rec: &Recording) -> Vec<String> {
    let n = rec.tracks.len();
    if n <= 1 {
        return vec!["-map".into(), "0:a:0".into(), "-ac".into(), "1".into()];
    }
    let inputs: String = (0..n).map(|i| format!("[0:a:{i}]")).collect();
    vec![
        "-filter_complex".into(),
        format!("{inputs}amix=inputs={n}:duration=longest:normalize=0,aresample=async=1[a]"),
        "-map".into(),
        "[a]".into(),
        "-ac".into(),
        "1".into(),
    ]
}

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

/// 16 kHz mono WAV for transcription. Large and disposable — deleted once the
/// transcript exists.
pub fn extract_for_asr(
    sc: &Sidecars,
    rec: &Recording,
    dest: &Path,
    on_progress: impl FnMut(f64),
) -> Result<()> {
    let mut args = vec!["-i".to_string(), rec.path.clone()];
    args.extend(mix_args(rec));
    args.extend([
        "-vn".into(),
        "-ar".into(),
        ASR_RATE.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        dest.to_string_lossy().into_owned(),
    ]);
    run_ffmpeg(sc, args, rec.duration, on_progress)
}

/// Small Opus copy kept for previewing a beat. Roughly 11 MB per hour, so a
/// 100 hour job costs about a gigabyte.
pub fn extract_preview(
    sc: &Sidecars,
    rec: &Recording,
    dest: &Path,
    on_progress: impl FnMut(f64),
) -> Result<()> {
    let mut args = vec!["-i".to_string(), rec.path.clone()];
    args.extend(mix_args(rec));
    args.extend([
        "-vn".into(),
        "-c:a".into(),
        "libopus".into(),
        "-b:a".into(),
        "24k".into(),
        "-application".into(),
        "voip".into(),
        dest.to_string_lossy().into_owned(),
    ]);
    run_ffmpeg(sc, args, rec.duration, on_progress)
}
