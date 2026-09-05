use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use regex::Regex;
use serde_json::Value;
use walkdir::WalkDir;

use crate::model::{Library, Recording, Session, Track};
use crate::sidecar::Sidecars;

const VIDEO_EXT: [&str; 4] = ["mp4", "mkv", "mov", "flv"];

/// OBS, ShadowPlay and Windows Game Bar all stamp filenames differently.
fn parse_stamp(name: &str) -> Option<DateTime<Local>> {
    let ymd = Regex::new(
        r"(\d{4})[-._](\d{2})[-._](\d{2})[ _-]+(\d{2})[-._](\d{2})(?:[-._](\d{2}))?",
    )
    .unwrap();
    let dmy =
        Regex::new(r"(\d{2})\.(\d{2})\.(\d{4})[ _-]+(\d{2})[-._](\d{2})(?:[-._](\d{2}))?").unwrap();

    let n = |c: &regex::Captures, i: usize| -> i32 {
        c.get(i).map_or(0, |m| m.as_str().parse().unwrap_or(0))
    };

    let (y, mo, d, h, mi, s) = if let Some(c) = ymd.captures(name) {
        (n(&c, 1), n(&c, 2), n(&c, 3), n(&c, 4), n(&c, 5), n(&c, 6))
    } else if let Some(c) = dmy.captures(name) {
        (n(&c, 3), n(&c, 2), n(&c, 1), n(&c, 4), n(&c, 5), n(&c, 6))
    } else {
        return None;
    };

    NaiveDate::from_ymd_opt(y, mo as u32, d as u32)
        .and_then(|date| date.and_hms_opt(h as u32, mi as u32, s as u32))
        .and_then(|naive| Local.from_local_datetime(&naive).single())
}

/// Stable across runs and machines: path basename plus byte size.
fn recording_id(path: &Path, size: u64) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let digest = blake3::hash(format!("{name}:{size}").as_bytes());
    digest.to_hex()[..12].to_string()
}

fn fps_of(stream: &Value) -> f64 {
    for key in ["avg_frame_rate", "r_frame_rate"] {
        if let Some(v) = stream.get(key).and_then(|v| v.as_str()) {
            if let Some((num, den)) = v.split_once('/') {
                let (n, d) = (num.parse::<f64>().unwrap_or(0.0), den.parse::<f64>().unwrap_or(0.0));
                if d > 0.0 && n > 0.0 {
                    return n / d;
                }
            }
        }
    }
    0.0
}

pub fn probe(sc: &Sidecars, path: &Path) -> Result<Recording> {
    let out = Command::new(&sc.ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .with_context(|| format!("running ffprobe on {}", path.display()))?;

    if !out.status.success() {
        bail!("ffprobe failed for {}: {}", path.display(), String::from_utf8_lossy(&out.stderr));
    }

    let data: Value = serde_json::from_slice(&out.stdout)?;
    let empty = vec![];
    let streams = data["streams"].as_array().unwrap_or(&empty);

    let duration = data["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    if duration <= 0.0 {
        bail!("{} has no usable duration", path.display());
    }

    let tracks: Vec<Track> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .map(|s| {
            let index = s["index"].as_u64().unwrap_or(0) as u32;
            Track {
                name: s["tags"]["title"]
                    .as_str()
                    .or_else(|| s["tags"]["name"].as_str())
                    .unwrap_or(&format!("track{index}"))
                    .to_string(),
                codec: s["codec_name"].as_str().unwrap_or("?").to_string(),
                channels: s["channels"].as_u64().unwrap_or(0) as u32,
                index,
            }
        })
        .collect();

    if tracks.is_empty() {
        bail!("{} has no audio", path.display());
    }
    let _ = streams.iter().find(|s| s["codec_type"] == "video").map(fps_of);

    let meta = std::fs::metadata(path)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();

    // OBS writes mtime when the file closes, so start is mtime minus runtime.
    let (start, stamp_source) = match parse_stamp(&name) {
        Some(dt) => (dt, "filename"),
        None => {
            let mtime: DateTime<Local> = meta.modified()?.into();
            (mtime - Duration::milliseconds((duration * 1000.0) as i64), "mtime")
        }
    };

    Ok(Recording {
        id: recording_id(path, meta.len()),
        path: path.to_string_lossy().into_owned(),
        name,
        duration,
        wall_start: start.format("%Y-%m-%dT%H:%M:%S").to_string(),
        stamp_source: stamp_source.to_string(),
        session_id: 0,
        session_offset: 0.0,
        global_offset: 0.0,
        tracks,
    })
}

fn start_of(r: &Recording) -> DateTime<Local> {
    chrono::NaiveDateTime::parse_from_str(&r.wall_start, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .and_then(|n| Local.from_local_datetime(&n).single())
        .unwrap_or_else(Local::now)
}

/// Groups recordings into sessions on wall-clock gaps, then assigns each one a
/// global offset measured in *content* seconds, not wall-clock: idle time
/// between recordings is not footage and must not shift later timestamps.
pub fn sessionise(mut recs: Vec<Recording>, gap_minutes: f64) -> Library {
    recs.sort_by_key(start_of);

    let gap = Duration::milliseconds((gap_minutes * 60_000.0) as i64);
    let mut sessions: Vec<Session> = Vec::new();
    let mut prev_end: Option<DateTime<Local>> = None;
    let mut global = 0.0;

    for rec in &mut recs {
        let start = start_of(rec);
        let split = match prev_end {
            None => true,
            Some(end) => start.signed_duration_since(end) > gap,
        };

        if split {
            sessions.push(Session {
                id: sessions.len(),
                label: start.format("%a %d %b %H:%M").to_string(),
                start: rec.wall_start.clone(),
                duration: 0.0,
                global_offset: global,
                recordings: Vec::new(),
            });
        }

        let s = sessions.last_mut().unwrap();
        rec.session_id = s.id;
        rec.session_offset = s.duration;
        rec.global_offset = global;

        s.recordings.push(rec.id.clone());
        s.duration += rec.duration;
        global += rec.duration;
        prev_end = Some(start + Duration::milliseconds((rec.duration * 1000.0) as i64));
    }

    Library {
        root: String::new(),
        scanned_at: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        total_duration: global,
        sessions,
        recordings: recs,
    }
}

pub fn find_videos(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| VIDEO_EXT.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    paths
}

pub fn scan(sc: &Sidecars, root: &Path, gap_minutes: f64) -> Result<(Library, Vec<String>)> {
    let paths = find_videos(root);
    if paths.is_empty() {
        bail!("no video files under {}", root.display());
    }

    let mut recs = Vec::new();
    let mut problems = Vec::new();
    for path in paths {
        match probe(sc, &path) {
            Ok(r) => recs.push(r),
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    if recs.is_empty() {
        bail!("found files but none could be read");
    }

    let mut lib = sessionise(recs, gap_minutes);
    lib.root = root.to_string_lossy().into_owned();
    Ok((lib, problems))
}
