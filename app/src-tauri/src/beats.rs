use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::model::{Beat, Recording, Segment};
use crate::sidecar::Sidecars;

#[derive(Debug, Clone)]
pub struct BeatConfig {
    pub model: PathBuf,
    pub port: u16,
    pub window_secs: f64,
    pub overlap_secs: f64,
    pub max_per_window: usize,
    pub temperature: f64,
    pub context: usize,
}

impl BeatConfig {
    pub fn new(model: impl Into<PathBuf>) -> Self {
        Self {
            model: model.into(),
            port: 8899,
            window_secs: 600.0,
            overlap_secs: 60.0,
            max_per_window: 6,
            temperature: 0.2,
            context: 16384,
        }
    }
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/v1/chat/completions", self.port)
    }
}

/// Owns a llama-server child process and shuts it down on drop, so a cancelled
/// job cannot leave several gigabytes of model resident in VRAM.
pub struct Llm {
    child: Option<Child>,
    cfg: BeatConfig,
}

impl Llm {
    pub fn start(sc: &Sidecars, cfg: BeatConfig) -> Result<Self> {
        sc.require(&sc.llama)?;
        if !cfg.model.is_file() {
            bail!("llm model missing: {}", cfg.model.display());
        }

        let child = Command::new(&sc.llama)
            .arg("-m").arg(&cfg.model)
            .args(["-ngl", "99"])
            .args(["-c", &cfg.context.to_string()])
            .args(["--port", &cfg.port.to_string()])
            .args(["--host", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("spawning llama-server")?;

        let me = Self { child: Some(child), cfg };
        me.wait_ready(Duration::from_secs(180))?;
        Ok(me)
    }

    /// Attaches to an already-running server, for development.
    pub fn attach(cfg: BeatConfig) -> Self {
        Self { child: None, cfg }
    }

    fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let health = format!("http://127.0.0.1:{}/health", self.cfg.port);
        let start = Instant::now();
        while start.elapsed() < timeout {
            if ureq::get(&health).timeout(Duration::from_secs(2)).call().is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        bail!("llama-server did not become ready within {timeout:?}")
    }

    fn ask(&self, system: &str, user: &str) -> Result<Value> {
        let body = json!({
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "temperature": self.cfg.temperature,
            "max_tokens": 700,
            // Qwen3 reasons before answering by default, which triples latency
            // for no gain on a task this mechanical.
            "chat_template_kwargs": {"enable_thinking": false},
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "beats", "schema": schema(self.cfg.max_per_window)}
            }
        });

        let resp: Value = ureq::post(&self.cfg.url())
            .timeout(Duration::from_secs(300))
            .send_json(body)?
            .into_json()?;

        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .context("llm response had no content")?;
        serde_json::from_str(content).context("llm returned malformed json")
    }
}

impl Drop for Llm {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// `minItems` is deliberately 0 but `maxItems` is capped: given an unbounded
/// array the model pads to the limit with filler, and given a floor it invents
/// events for stretches where nothing happened.
fn schema(max: usize) -> Value {
    json!({
        "type": "object",
        "properties": {
            "beats": {
                "type": "array",
                "minItems": 0,
                "maxItems": max,
                "items": {
                    "type": "object",
                    "properties": {
                        "t": {"type": "string", "pattern": "^[0-9]{1,2}:[0-9]{2}$"},
                        "text": {"type": "string", "maxLength": 90}
                    },
                    "required": ["t", "text"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["beats"]
    })
}

const SYSTEM: &str = "You reconstruct what happened in a video game from the players' voice chat. \
You are writing a match log, not a conversation log.";

fn hms(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn parse_hms(t: &str) -> Option<f64> {
    let (m, s) = t.split_once(':')?;
    Some(m.trim().parse::<f64>().ok()? * 60.0 + s.trim().parse::<f64>().ok()?)
}

fn prompt(lines: &str, t0: &str, t1: &str) -> String {
    format!(
        "Voice chat from a gameplay recording, {t0} to {t1}. You hear the players; you cannot see the screen.

{lines}

Write a match log: what happened in the game world.

  \"I'm dead\"            -> \"they died\"
  \"double kill\"         -> \"they got a double kill\"
  \"push mid\"            -> \"the team pushed mid\"
  \"Haze wasted unstop\"  -> \"Haze wasted her unstop ult\"

Report the event the words imply, never the fact that someone spoke. Do not
write \"asked\", \"said\", \"mentioned\", \"noted\", \"praised\", \"joked\".

Never use \"a player\", \"the player\", \"Player\" or \"Someone\" as a subject. If you
do not know who acted, write \"they\" or \"the team\". Use a name only when the
transcript actually says that name.

Each beat is a clause with a subject and a past-tense verb, 5 to 12 words.
Prefer a few strong beats over many weak ones. Spread them across {t0}-{t1}.
t copied exactly from a timestamp above."
    )
}

/// Two beats describing the same moment, produced by neighbouring windows
/// overlapping. Time proximity alone is too eager; wording alone misses
/// rephrasings, so require both.
fn duplicates(a: &Beat, b: &Beat) -> bool {
    if (a.global - b.global).abs() > 45.0 {
        return false;
    }
    let norm = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(str::to_string)
            .collect()
    };
    let (x, y) = (norm(&a.text), norm(&b.text));
    if x.is_empty() || y.is_empty() {
        return true;
    }
    let shared = x.iter().filter(|w| y.contains(w)).count();
    shared * 2 >= x.len().min(y.len())
}

pub fn run(
    llm: &Llm,
    rec: &Recording,
    segments: &[Segment],
    mut on_progress: impl FnMut(f64),
) -> Result<Vec<Beat>> {
    let cfg = &llm.cfg;
    let step = cfg.window_secs - cfg.overlap_secs;
    let mut out: Vec<Beat> = Vec::new();
    let mut start = 0.0;

    while start < rec.duration {
        let end = (start + cfg.window_secs).min(rec.duration);
        let window: Vec<&Segment> =
            segments.iter().filter(|s| s.start >= start && s.start < end).collect();

        if window.len() >= 3 {
            let lines = window
                .iter()
                .map(|s| format!("[{}] {}", hms(s.start), s.text))
                .collect::<Vec<_>>()
                .join("\n");

            match llm.ask(SYSTEM, &prompt(&lines, &hms(start), &hms(end))) {
                Ok(v) => {
                    for b in v["beats"].as_array().unwrap_or(&vec![]) {
                        let (Some(t), Some(text)) = (b["t"].as_str(), b["text"].as_str()) else {
                            continue;
                        };
                        // The model will occasionally invent a plausible-looking
                        // timestamp outside the window it was shown.
                        let Some(secs) = parse_hms(t).filter(|s| *s >= start && *s <= end) else {
                            continue;
                        };
                        let beat = Beat {
                            global: rec.global_offset + secs,
                            recording_id: rec.id.clone(),
                            offset: secs,
                            text: text.trim().to_string(),
                            starred: false,
                            source: "llm".into(),
                        };
                        if !out.iter().any(|o| duplicates(o, &beat)) {
                            out.push(beat);
                        }
                    }
                }
                Err(e) => eprintln!("window {} failed: {e}", hms(start)),
            }
        }

        on_progress((end / rec.duration).clamp(0.0, 1.0));
        if end >= rec.duration {
            break;
        }
        start += step;
    }

    out.sort_by(|a, b| a.global.total_cmp(&b.global));
    Ok(out)
}
