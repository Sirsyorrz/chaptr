//! Headless driver for the same pipeline the app uses. Exists so the long
//! stages can be run and timed without a window in the way.

use std::path::PathBuf;

use anyhow::{bail, Result};
use chaptr::model::Library;
use chaptr::sidecar::Sidecars;
use chaptr::workspace::{self, Workspace};
use chaptr::{audio, scan};

fn hms(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn ws_for(footage: &str) -> Workspace {
    Workspace::new(PathBuf::from(footage).join(".chaptr"))
}

fn cmd_scan(folder: &str, gap: f64) -> Result<()> {
    let sc = Sidecars::discover();
    sc.require(&sc.ffprobe)?;

    let (lib, problems) = scan::scan(&sc, &PathBuf::from(folder), gap)?;
    let ws = ws_for(folder);
    ws.prepare()?;
    workspace::write_json(&ws.library(), &lib)?;

    for s in &lib.sessions {
        println!(
            "session {:>2}  {}  {:>3} files  {}",
            s.id,
            s.label,
            s.recordings.len(),
            hms(s.duration)
        );
    }
    println!(
        "\n{} recordings, {} sessions, {} total",
        lib.recordings.len(),
        lib.sessions.len(),
        hms(lib.total_duration)
    );
    let guessed = lib.recordings.iter().filter(|r| r.stamp_source == "mtime").count();
    if guessed > 0 {
        println!("{guessed} had no timestamp in the filename; used file mtime");
    }
    for p in &problems {
        eprintln!("skipped {p}");
    }
    println!("wrote {}", ws.library().display());
    Ok(())
}

fn cmd_audio(folder: &str, limit: usize) -> Result<()> {
    let sc = Sidecars::discover();
    sc.require(&sc.ffmpeg)?;
    let ws = ws_for(folder);
    ws.prepare()?;
    let lib: Library = workspace::read_json(&ws.library())?;

    for rec in lib.recordings.iter().take(limit) {
        let dest = ws.preview(&rec.id);
        if dest.exists() {
            println!("skip {} (preview exists)", rec.name);
            continue;
        }
        let t = std::time::Instant::now();
        print!("{} ... ", rec.name);
        audio::extract_preview(&sc, rec, &dest, |_| {})?;
        let size = std::fs::metadata(&dest)?.len() as f64 / 1e6;
        println!(
            "{:.1}s  {:.1} MB  {:.0}x realtime",
            t.elapsed().as_secs_f64(),
            size,
            rec.duration / t.elapsed().as_secs_f64()
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str).unwrap_or("");
    let folder = args.get(2).cloned().unwrap_or_default();

    match sub {
        "scan" if !folder.is_empty() => cmd_scan(&folder, 45.0),
        "audio" if !folder.is_empty() => {
            let limit = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
            cmd_audio(&folder, limit)
        }
        "doctor" => {
            let missing = Sidecars::discover().missing();
            if missing.is_empty() {
                println!("all sidecars present");
            } else {
                println!("missing: {}", missing.join(", "));
            }
            Ok(())
        }
        _ => bail!("usage: chaptr-cli <scan|audio|doctor> <folder> [limit]"),
    }
}
