//! Headless driver for the same pipeline the app uses. Exists so the long
//! stages can be run and timed without a window in the way.

use std::path::PathBuf;

use anyhow::{bail, Result};
use chaptr::asr::{self, AsrConfig};
use chaptr::model::{Library, Recording, Transcript};
use chaptr::sidecar::Sidecars;
use chaptr::workspace::{self, Workspace};
use chaptr::beats::{BeatConfig, Llm};
use chaptr::model::Beat;
use chaptr::{audio, beats, scan};

fn hms(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn ws_for(footage: &str) -> Workspace {
    Workspace::new(PathBuf::from(footage).join(".chaptr"))
}

fn models() -> AsrConfig {
    let dir = std::env::var("CHAPTR_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_home().join("src/whisper-models"));
    AsrConfig::new(
        dir.join("ggml-large-v3-turbo-q5_0.bin"),
        dir.join("ggml-silero-v5.1.2.bin"),
    )
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
}

fn load(folder: &str) -> Result<(Workspace, Library)> {
    let ws = ws_for(folder);
    let lib: Library = workspace::read_json(&ws.library())?;
    Ok((ws, lib))
}

fn cmd_scan(folder: &str, gap: f64) -> Result<()> {
    let sc = Sidecars::discover();
    sc.require(&sc.ffprobe)?;

    let (lib, problems) = scan::scan(&sc, &PathBuf::from(folder), gap)?;
    let ws = ws_for(folder);
    ws.prepare()?;
    workspace::write_json(&ws.library(), &lib)?;

    println!(
        "{} recordings, {} sessions, {} total",
        lib.recordings.len(),
        lib.sessions.len(),
        hms(lib.total_duration)
    );
    for p in &problems {
        eprintln!("skipped {p}");
    }
    Ok(())
}

/// Works out which audio track carries conversation, by transcribing short
/// samples of each. Runs once per recording layout, not once per file.
fn cmd_tracks(folder: &str) -> Result<()> {
    let sc = Sidecars::discover();
    let (ws, lib) = load(folder)?;
    let cfg = models();

    let rec: &Recording = lib
        .recordings
        .iter()
        .max_by(|a, b| a.duration.total_cmp(&b.duration))
        .unwrap();
    println!("probing {} ({} tracks)\n", rec.name, rec.tracks.len());

    let spots = [0.25, 0.5, 0.75];
    let window = 60.0;
    for (i, t) in rec.tracks.iter().enumerate() {
        let mut segs = Vec::new();
        for f in spots {
            let tmp = ws.scratch(&format!("probe{i}"));
            audio::sample_track(&sc, rec, i, rec.duration * f, window, &tmp)?;
            segs.extend(asr::transcribe(&sc, &cfg, &tmp, "probe", |_| {})?);
            let _ = std::fs::remove_file(&tmp);
        }
        let score = asr::speech_score(&segs, window * spots.len() as f64);
        let sample: String = segs.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        println!(
            "track {i} [{}] {:>6.0} wpm  {}",
            t.name,
            score,
            &sample.chars().take(90).collect::<String>()
        );
    }
    Ok(())
}

fn cmd_transcribe(folder: &str, limit: usize) -> Result<()> {
    let sc = Sidecars::discover();
    let (ws, lib) = load(folder)?;
    let cfg = models();
    let track: usize = std::env::var("CHAPTR_TRACK").ok().and_then(|v| v.parse().ok()).unwrap_or(0);

    for rec in lib.recordings.iter().take(limit) {
        let dest = ws.transcript(&rec.id);
        if dest.exists() {
            println!("skip {}", rec.name);
            continue;
        }
        let t = std::time::Instant::now();
        println!("{} ({})", rec.name, hms(rec.duration));

        let wav = ws.scratch(&format!("{}.t{track}", rec.id));
        audio::sample_track(&sc, rec, track, 0.0, rec.duration, &wav)?;
        let extracted = t.elapsed().as_secs_f64();

        let segments = asr::transcribe(&sc, &cfg, &wav, "all", |p| {
            print!("\r  {:.0}%   ", p * 100.0);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        })?;
        let _ = std::fs::remove_file(&wav);

        workspace::write_json(
            &dest,
            &Transcript {
                recording_id: rec.id.clone(),
                duration: rec.duration,
                model: cfg.model.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                segments: segments.clone(),
            },
        )?;

        let el = t.elapsed().as_secs_f64();
        println!(
            "\r  {} segments, audio {:.0}s, total {:.0}s, {:.0}x realtime",
            segments.len(),
            extracted,
            el,
            rec.duration / el
        );
    }
    Ok(())
}

fn cmd_beats(folder: &str, limit: usize) -> Result<()> {
    let (ws, lib) = load(folder)?;
    let model = std::env::var("CHAPTR_LLM")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_home().join("src/llm-models/Qwen3-14B-Q4_K_M.gguf"));
    let cfg = BeatConfig::new(model);

    // Reuse a server that is already up; otherwise own one for this run.
    let health = format!("http://127.0.0.1:{}/health", cfg.port);
    let llm = if ureq::get(&health).timeout(std::time::Duration::from_secs(2)).call().is_ok() {
        println!("using running llama-server on :{}", cfg.port);
        Llm::attach(cfg)
    } else {
        println!("starting llama-server ...");
        Llm::start(&Sidecars::discover(), cfg)?
    };

    let mut all: Vec<Beat> = Vec::new();
    for rec in lib.recordings.iter().take(limit) {
        let path = ws.transcript(&rec.id);
        if !path.exists() {
            continue;
        }
        let tr: Transcript = workspace::read_json(&path)?;
        let t = std::time::Instant::now();
        let got = beats::run(&llm, rec, &tr.segments, |_| {})?;
        println!(
            "{:<28} {} -> {:>3} beats in {:.0}s",
            rec.name,
            hms(rec.duration),
            got.len(),
            t.elapsed().as_secs_f64()
        );
        all.extend(got);
    }

    all.sort_by(|a, b| a.global.total_cmp(&b.global));
    workspace::write_json(&ws.beats(), &serde_json::json!({ "beats": all }))?;
    println!("\n{} beats -> {}", all.len(), ws.beats().display());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str).unwrap_or("");
    let folder = args.get(2).cloned().unwrap_or_default();
    let limit = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);

    match sub {
        "scan" if !folder.is_empty() => cmd_scan(&folder, 45.0),
        "tracks" if !folder.is_empty() => cmd_tracks(&folder),
        "transcribe" if !folder.is_empty() => cmd_transcribe(&folder, limit),
        "beats" if !folder.is_empty() => cmd_beats(&folder, limit),
        "doctor" => {
            let missing = Sidecars::discover().missing();
            let cfg = models();
            println!(
                "sidecars: {}",
                if missing.is_empty() { "ok".into() } else { format!("missing {}", missing.join(", ")) }
            );
            for (n, p) in [("asr model", &cfg.model), ("vad model", &cfg.vad_model)] {
                println!("{n}: {}", if p.is_file() { "ok" } else { "MISSING" });
            }
            Ok(())
        }
        _ => bail!("usage: chaptr-cli <scan|tracks|transcribe|beats|doctor> <folder> [limit]"),
    }
}
