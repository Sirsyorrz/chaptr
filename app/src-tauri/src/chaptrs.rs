use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::model::{Chaptr, Recording, Segment};
use crate::sidecar::Sidecars;

use crate::prefs::{Engine, Prefs, Provider};

#[derive(Debug, Clone)]
pub struct ChaptrConfig {
    pub model: PathBuf,
    pub port: u16,
    pub window_secs: f64,
    pub overlap_secs: f64,
    pub max_per_window: usize,
    pub temperature: f64,
    pub context: usize,
    /// What the footage is, e.g. "a match of Deadlock" or "a recorded meeting".
    /// Nothing else in the pipeline is game-specific; this is the only knob.
    pub subject: String,
    /// Optional names and jargon, to stop the model mangling them.
    pub notes: String,
    /// A floor of 2 is what stops whole windows coming back empty: given the
    /// option of returning nothing, the model takes it about half the time.
    pub min_per_window: usize,
    /// Empty for a local model; otherwise a cloud endpoint.
    pub remote: Option<Remote>,
}

#[derive(Debug, Clone)]
pub struct Remote {
    pub provider: Provider,
    pub model: String,
    pub key: String,
    pub base_url: String,
}

impl Remote {
    pub fn from(p: &Prefs) -> Option<Self> {
        if p.engine != Engine::Cloud || p.key().is_empty() {
            return None;
        }
        Some(Self {
            provider: p.cloud_provider,
            model: p.cloud_model.clone(),
            key: p.key().to_string(),
            base_url: p.cloud_base_url.clone(),
        })
    }

    fn endpoint(&self) -> String {
        if !self.base_url.trim().is_empty() {
            return format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        }
        match self.provider {
            Provider::Anthropic => "https://api.anthropic.com/v1/messages".into(),
            Provider::Openai => "https://api.openai.com/v1/chat/completions".into(),
            Provider::Google => {
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".into()
            }
            Provider::Compatible => "http://127.0.0.1:8899/v1/chat/completions".into(),
        }
    }
}

impl ChaptrConfig {
    pub fn new(model: impl Into<PathBuf>) -> Self {
        Self {
            model: model.into(),
            port: 8899,
            window_secs: 600.0,
            overlap_secs: 60.0,
            max_per_window: 6,
            temperature: 0.2,
            context: 16384,
            subject: "a video game session".into(),
            notes: String::new(),
            min_per_window: 2,
            remote: None,
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
    cfg: ChaptrConfig,
}

impl Llm {
    pub fn start(sc: &Sidecars, cfg: ChaptrConfig) -> Result<Self> {
        // A cloud endpoint needs no local server and no model file.
        if cfg.remote.is_some() {
            return Ok(Self { child: None, cfg });
        }
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
    pub fn attach(cfg: ChaptrConfig) -> Self {
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
        match &self.cfg.remote {
            Some(remote) => self.ask_remote(remote, system, user),
            None => self.ask_local(system, user),
        }
    }

    fn ask_local(&self, system: &str, user: &str) -> Result<Value> {
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
                "json_schema": {
                    "name": "chaptrs",
                    "schema": schema(self.cfg.min_per_window, self.cfg.max_per_window)
                }
            }
        });

        let resp: Value = ureq::post(&self.cfg.url())
            .timeout(Duration::from_secs(300))
            .send_json(body)?
            .into_json()?;
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .context("llm response had no content")?;
        parse_lenient(content)
    }

    fn ask_remote(&self, remote: &Remote, system: &str, user: &str) -> Result<Value> {
        let schema = schema(self.cfg.min_per_window, self.cfg.max_per_window);
        let endpoint = remote.endpoint();

        let request = if remote.provider == Provider::Anthropic
            && remote.base_url.trim().is_empty()
        {
            // Anthropic has its own shape; a tool with the schema is how it is
            // made to return structured output.
            ureq::post(&endpoint)
                .set("x-api-key", &remote.key)
                .set("anthropic-version", "2023-06-01")
                .set("content-type", "application/json")
                .timeout(Duration::from_secs(300))
                .send_json(json!({
                    "model": remote.model,
                    "max_tokens": 1500,
                    "temperature": self.cfg.temperature,
                    "system": system,
                    "tool_choice": {"type": "tool", "name": "chaptrs"},
                    "tools": [{
                        "name": "chaptrs",
                        "description": "Record what happened in this stretch of recording.",
                        "input_schema": schema
                    }],
                    "messages": [{"role": "user", "content": user}]
                }))
        } else {
            ureq::post(&endpoint)
                .set("authorization", &format!("Bearer {}", remote.key))
                .set("content-type", "application/json")
                .timeout(Duration::from_secs(300))
                .send_json(json!({
                    "model": remote.model,
                    "temperature": self.cfg.temperature,
                    "max_tokens": 1500,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": format!(
                            "{user}\n\nReply with JSON only, matching: {}",
                            serde_json::to_string(&schema).unwrap_or_default()
                        )}
                    ],
                    "response_format": {"type": "json_object"}
                }))
        };

        let resp: Value = match request {
            Ok(r) => r.into_json()?,
            Err(ureq::Error::Status(code, r)) => {
                let detail = r.into_string().unwrap_or_default();
                bail!("{} returned {code}: {}", endpoint, detail.chars().take(300).collect::<String>());
            }
            Err(e) => bail!("{endpoint}: {e}"),
        };

        // Anthropic returns the tool input; everyone else returns message text.
        if let Some(input) = resp["content"]
            .as_array()
            .and_then(|c| c.iter().find(|b| b["type"] == "tool_use"))
            .map(|b| b["input"].clone())
        {
            return Ok(input);
        }
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .with_context(|| format!("unexpected response: {}", resp.to_string().chars().take(200).collect::<String>()))?;
        parse_lenient(content)
    }
}

/// Cloud models wrap JSON in prose or code fences even when told not to.
fn parse_lenient(text: &str) -> Result<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        return Ok(v);
    }
    let start = text.find('{').context("no json in response")?;
    let end = text.rfind('}').context("no json in response")?;
    serde_json::from_str(&text[start..=end]).context("llm returned malformed json")
}

impl Drop for Llm {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Both bounds matter. Without `maxItems` the model pads to whatever length it
/// is allowed; without `minItems` it returns an empty array for half of all
/// windows, which loses far more than the occasional weak beat costs.
fn schema(min: usize, max: usize) -> Value {
    json!({
        "type": "object",
        "properties": {
            "beats": {
                "type": "array",
                "minItems": min,
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

const SYSTEM: &str = "You index recordings so an editor can find moments again later. \
You log what happened, concisely and concretely.";

fn hms(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn parse_hms(t: &str) -> Option<f64> {
    let (m, s) = t.split_once(':')?;
    Some(m.trim().parse::<f64>().ok()? * 60.0 + s.trim().parse::<f64>().ok()?)
}

fn prompt(cfg: &ChaptrConfig, lines: &str, t0: &str, t1: &str) -> String {
    let mut context = format!("This recording is {}.", cfg.subject);
    if !cfg.notes.trim().is_empty() {
        context.push_str(&format!(" Names and terms you may hear: {}.", cfg.notes.trim()));
    }

    format!(
        "Audio from a recording, {t0} to {t1}. {context}
You hear people talking. You cannot see the screen, so infer what is happening.

{lines}

Log the moments an editor would want to find again: things that happened,
decisions made, and exchanges that are genuinely funny or memorable.

Write the event, not the speech act:
  \"I'm dead\"      -> \"they died\"
  \"double kill\"   -> \"they got a double kill\"
  \"let's regroup\" -> \"the team regrouped\"
For a funny or notable exchange, log what it was ABOUT:
  -> \"they argued about who caused the wipe\"
Do not quote a line verbatim as if it were the event.

Use a name whenever the transcript says one. \"a player\" or \"the team\" is fine
when you cannot tell who acted. Avoid \"someone\".

Each beat: a subject and a past-tense verb, 5 to 12 words.
Give {min} to {max} beats, spread across {t0}-{t1}. Return fewer only if this
stretch is almost silent. t copied exactly from a timestamp above.",
        min = cfg.min_per_window + 1,
        max = cfg.max_per_window,
    )
}

/// Two beats describing the same moment, produced by neighbouring windows
/// overlapping. Time proximity alone is too eager; wording alone misses
/// rephrasings, so require both.
fn duplicates(a: &Chaptr, b: &Chaptr) -> bool {
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
) -> Result<Vec<Chaptr>> {
    let cfg = &llm.cfg;
    let step = cfg.window_secs - cfg.overlap_secs;
    let mut out: Vec<Chaptr> = Vec::new();
    let mut start = 0.0;

    while start < rec.duration {
        let end = (start + cfg.window_secs).min(rec.duration);
        let window: Vec<&Segment> =
            segments.iter().filter(|s| s.start >= start && s.start < end).collect();

        if window.len() >= 3 {
            // Speaker markers are deliberately NOT passed to the model. Doing so
            // measured worse across 4.35 hours: it swaps real names for "the
            // host" and "a friend" and drifts into describing the conversation.
            // The labels still earn their place in the transcript view.
            let lines = window
                .iter()
                .map(|s| format!("[{}] {}", hms(s.start), s.text))
                .collect::<Vec<_>>()
                .join("\n");

            match llm.ask(SYSTEM, &prompt(cfg, &lines, &hms(start), &hms(end))) {
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
                        let beat = Chaptr {
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
