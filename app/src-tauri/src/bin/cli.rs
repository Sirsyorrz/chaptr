//! Headless driver for the same pipeline the app uses. Exists so the long
//! stages can be run and timed without a window in the way.

use std::path::PathBuf;

use anyhow::{bail, Result};
use chaptr::asr::{self, AsrConfig};
use chaptr::model::{Library, Recording, Transcript};
use chaptr::sidecar::Sidecars;
use chaptr::workspace::{self, Workspace};
use chaptr::chaptrs::{ChaptrConfig, Llm};
use chaptr::model::Chaptr;
use chaptr::tracks::{self, Layout, Settings};
use chaptr::transcribe;
use chaptr::project;
use chaptr::{audio, chaptrs, scan};

fn hms(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Accepts a project id, or a folder/clip path which is opened as a project.
fn resolve(target: &str) -> Result<(String, Workspace)> {
    if project::get(target).is_some() {
        return Ok((target.to_string(), project::workspace(target)));
    }
    let p = project::open(vec![target.to_string()])?;
    project::adopt_legacy(&p.sources, &p.id);
    Ok((p.id.clone(), project::workspace(&p.id)))
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

fn load(target: &str) -> Result<(Workspace, Library)> {
    let (_, ws) = resolve(target)?;
    let lib: Library = workspace::read_json(&ws.library())?;
    Ok((ws, lib))
}

fn cmd_scan(sources: &[String], gap: f64) -> Result<()> {
    let sc = Sidecars::discover();
    sc.require(&sc.ffprobe)?;

    let p = project::open(sources.to_vec())?;
    if let Some(old) = project::adopt_legacy(&p.sources, &p.id) {
        println!("adopted existing data from {}", old.display());
    }
    let ws = project::workspace(&p.id);
    let (lib, problems) = scan::scan(&sc, &p.sources, gap)?;
    ws.prepare()?;
    workspace::write_json(&ws.library(), &lib)?;

    println!(
        "{} recordings, {} sessions, {} total",
        lib.recordings.len(),
        lib.sessions.len(),
        hms(lib.total_duration)
    );
    project::touch(&p.id, lib.recordings.len(), lib.total_duration);
    for pr in &problems {
        eprintln!("skipped {pr}");
    }
    println!("project {} [{}]  {}", p.name, p.id, ws.root.display());
    Ok(())
}

/// Works out what each audio track carries, once per distinct layout rather
/// than once per recording.
fn cmd_tracks(folder: &str) -> Result<()> {
    let sc = Sidecars::discover();
    let (ws, lib) = load(folder)?;
    let cfg = models();
    let mut settings: Settings =
        workspace::maybe_json(&ws.settings()).unwrap_or_default();

    let mut out = Vec::new();
    for (sig, rec, count) in tracks::layouts(&lib) {
        println!("layout {sig:<28} {count:>4} recordings   probing {}", rec.name);
        let probes = tracks::probe_layout(&sc, &cfg, &ws, rec)?;
        for p in &probes {
            println!(
                "  track {} [{:<14}] {:>5.0} wpm  {:?}{}  {}",
                p.index,
                p.name,
                p.words_per_minute,
                p.role,
                if p.confident { "" } else { " (confirm)" },
                p.sample.chars().take(60).collect::<String>()
            );
        }
        settings
            .roles
            .insert(sig.clone(), probes.iter().map(|p| p.role).collect());
        out.push(Layout {
            signature: sig,
            track_count: probes.len(),
            recordings: count,
            example: rec.name.clone(),
            tracks: probes,
        });
        println!();
    }

    workspace::write_json(&ws.settings(), &settings)?;
    println!("wrote {}", ws.settings().display());
    println!("edit roles there, or in the app, before transcribing");
    Ok(())
}

fn cmd_transcribe(folder: &str, limit: usize) -> Result<()> {
    let sc = Sidecars::discover();
    let (ws, lib) = load(folder)?;
    let cfg = models();
    let settings: Settings = workspace::maybe_json(&ws.settings()).unwrap_or_default();
    if settings.roles.is_empty() {
        bail!("no track roles yet: run `chaptr-cli tracks {folder}` first");
    }

    for rec in lib.recordings.iter().take(limit) {
        let dest = ws.transcript(&rec.id);
        if dest.exists() {
            println!("skip {}", rec.name);
            continue;
        }
        let Some(roles) = settings.roles_for(rec) else {
            eprintln!("no roles for layout {} ({})", tracks::signature(rec), rec.name);
            continue;
        };
        let plan = transcribe::plan(roles)?;
        let t = std::time::Instant::now();
        println!("{} ({})  {:?}", rec.name, hms(rec.duration), plan);

        let tr = transcribe::run(&sc, &cfg, &ws, rec, roles, &dest, |p| {
            print!("\r  {:.0}%   ", p * 100.0);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        })?;

        let el = t.elapsed().as_secs_f64();
        let hosts = tr.segments.iter().filter(|s| s.who == "host").count();
        let friends = tr.segments.iter().filter(|s| s.who == "friend").count();
        println!(
            "\r  {} segments ({} host, {} friend), {:.0}s, {:.0}x realtime",
            tr.segments.len(),
            hosts,
            friends,
            el,
            rec.duration / el
        );
    }
    ws.clear_scratch();
    Ok(())
}

fn cmd_chaptrs(folder: &str, limit: usize) -> Result<()> {
    let (ws, lib) = load(folder)?;
    let model = std::env::var("CHAPTR_LLM")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_home().join("src/llm-models/Qwen3-14B-Q4_K_M.gguf"));
    let cfg = ChaptrConfig::new(model);

    // Reuse a server that is already up; otherwise own one for this run.
    let health = format!("http://127.0.0.1:{}/health", cfg.port);
    let llm = if ureq::get(&health).timeout(std::time::Duration::from_secs(2)).call().is_ok() {
        println!("using running llama-server on :{}", cfg.port);
        Llm::attach(cfg)
    } else {
        println!("starting llama-server ...");
        Llm::start(&Sidecars::discover(), cfg)?
    };

    let mut all: Vec<Chaptr> = Vec::new();
    for rec in lib.recordings.iter().take(limit) {
        let path = ws.transcript(&rec.id);
        if !path.exists() {
            continue;
        }
        let tr: Transcript = workspace::read_json(&path)?;
        let t = std::time::Instant::now();
        let got = chaptrs::run(&llm, rec, &tr.segments, |_| {})?;
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
    workspace::write_json(&ws.chaptrs(), &serde_json::json!({ "chaptrs": all }))?;
    println!("\n{} chaptrs -> {}", all.len(), ws.chaptrs().display());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str).unwrap_or("");
    let folder = args.get(2).cloned().unwrap_or_default();
    let limit = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);

    match sub {
        "resolve" => {
            let what = args.get(2).map(String::as_str).unwrap_or("status");
            match what {
                "install" => println!("installed {}", chaptr::resolve::install()?.display()),
                "goto" => {
                    let file = args.get(3).cloned().unwrap_or_default();
                    let at: f64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    chaptr::resolve::goto(&file, at)?;
                    println!(
                        "wrote {}: {}",
                        chaptr::resolve::request_file().display(),
                        std::fs::read_to_string(chaptr::resolve::request_file())?
                    );
                }
                _ => println!(
                    "script installed: {}\nscripts dir: {}\nrequest file: {}",
                    chaptr::resolve::installed(),
                    chaptr::resolve::scripts_dir().unwrap_or_default().display(),
                    chaptr::resolve::request_file().display()
                ),
            }
            Ok(())
        }

        "forget" if !folder.is_empty() => {
            let name = project::get(&folder).map(|p| p.name).unwrap_or_default();
            project::forget(&folder, true)?;
            println!("deleted {folder} {name}");
            Ok(())
        }

        "projects" => {
            for p in project::list() {
                println!(
                    "{}  {:<28} {:>3} recordings  {}",
                    p.id, p.name, p.recordings, hms(p.duration)
                );
            }
            Ok(())
        }
        "scan" if !folder.is_empty() => cmd_scan(&args[2..], 45.0),
        "tracks" if !folder.is_empty() => cmd_tracks(&folder),
        "transcribe" if !folder.is_empty() => cmd_transcribe(&folder, limit),
        "chaptrs" | "beats" if !folder.is_empty() => cmd_chaptrs(&folder, limit),
        "dump" if !folder.is_empty() => {
            let (id, ws) = resolve(&folder)?;
            let src = if ws.edits().exists() { ws.edits() } else { ws.chaptrs() };
            println!("project {id}\n  reading {}", src.display());
            let raw: serde_json::Value = workspace::read_json(&src)?;
            let key = if raw["chaptrs"].is_array() { "chaptrs" } else { "beats" };
            let list: Vec<chaptr::model::Chaptr> =
                serde_json::from_value(raw[key].clone())?;
            println!("  key {key}, {} chaptrs", list.len());
            for c in list.iter().take(3) {
                println!("    {:.0}s {}", c.global, c.text);
            }
            Ok(())
        }
        "save" if !folder.is_empty() => {
            let (id, ws) = resolve(&folder)?;
            let p = project::get(&id).unwrap();
            let dest = PathBuf::from(args.get(3).cloned().unwrap_or_else(|| format!("{}.chaptr", p.name)));
            let size = chaptr::bundle::save(&ws, &p.name, &p.sources, &dest)?;
            println!("wrote {} ({:.1} KB)", dest.display(), size as f64 / 1024.0);
            Ok(())
        }
        "open" if !folder.is_empty() => {
            let b = chaptr::bundle::read(&PathBuf::from(&folder))?;
            println!(
                "{}  v{}  {} sources  {} transcripts  library={}  chaptrs={}",
                b.name,
                b.version,
                b.sources.len(),
                b.transcripts.len(),
                b.library.as_ref().map_or(0, |l| l.recordings.len()),
                b.chaptrs["chaptrs"].as_array().map_or(0, |a| a.len()),
            );
            Ok(())
        }
        "models" => {
            let dir = std::env::var("CHAPTR_MODELS").map(PathBuf::from).unwrap_or_else(|_| {
                std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("models"))).unwrap_or_default()
            });
            if let Some(gb) = chaptr::catalogue::vram_gb() {
                println!("{gb} GB VRAM detected\n");
            }
            for e in chaptr::catalogue::ENTRIES {
                let path = dir.join(e.file);
                let have = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                println!(
                    "{:14} {:<32} {:>7.1} GB  {}",
                    e.id,
                    e.name,
                    e.bytes as f64 / 1e9,
                    if have > 0 { "installed" } else { "not installed" }
                );
            }
            Ok(())
        }
        "get" if !folder.is_empty() => {
            let e = chaptr::catalogue::find(&folder).ok_or_else(|| anyhow::anyhow!("unknown model"))?;
            let dir = std::env::var("CHAPTR_MODELS").map(PathBuf::from).unwrap_or_else(|_| {
                std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("models"))).unwrap_or_default()
            });
            let dest = dir.join(e.file);
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let start = std::time::Instant::now();
            let n = chaptr::download::fetch(e.url, &dest, &cancel, |got, total| {
                if total > 0 {
                    print!("\r  {:.0}%  {:.0} MB/s   ", 100.0 * got as f64 / total as f64,
                        got as f64 / 1e6 / start.elapsed().as_secs_f64().max(0.001));
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            })?;
            println!("\r  {} -> {} ({:.1} MB in {:.0}s)", e.id, dest.display(), n as f64 / 1e6, start.elapsed().as_secs_f64());
            Ok(())
        }
        "urlcheck" => {
            for e in chaptr::catalogue::ENTRIES {
                match ureq::head(e.url).timeout(std::time::Duration::from_secs(30)).call() {
                    Ok(r) => println!(
                        "{:14} {} {}",
                        e.id,
                        r.status(),
                        r.header("content-length").unwrap_or("?")
                    ),
                    Err(err) => println!("{:14} FAILED {err}", e.id),
                }
            }
            Ok(())
        }
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
        _ => bail!("usage: chaptr-cli <scan|tracks|transcribe|chaptrs|projects|forget|doctor> <folder-or-clip...> [limit]"),
    }
}
