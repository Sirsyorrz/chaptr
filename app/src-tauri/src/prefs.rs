use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::project::data_root;
use crate::workspace;

/// Machine-level preferences: model choices, API keys, hardware.
///
/// Deliberately *not* part of a project bundle. A `.chaptr` file is meant to be
/// handed to someone else, and an API key must never travel with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    pub whisper_model: String,
    pub language: String,
    /// 0-1. Higher skips more quiet audio; too high and mumbled lines vanish.
    pub vad_threshold: f64,

    pub engine: Engine,
    pub local_model: String,
    pub cloud_provider: Provider,
    pub cloud_model: String,
    pub cloud_base_url: String,
    pub anthropic_key: String,
    pub openai_key: String,
    pub google_key: String,
    /// Requests in flight against a cloud API. Local runs stay at 1.
    pub parallel: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Local,
    Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    Openai,
    Google,
    /// Anything speaking the OpenAI chat format: OpenRouter, Groq, a local server.
    Compatible,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            whisper_model: "ggml-large-v3-turbo-q5_0.bin".into(),
            language: "en".into(),
            vad_threshold: 0.5,
            engine: Engine::Local,
            local_model: "Qwen3-14B-Q4_K_M.gguf".into(),
            cloud_provider: Provider::Anthropic,
            cloud_model: "claude-sonnet-4-20250514".into(),
            cloud_base_url: String::new(),
            anthropic_key: String::new(),
            openai_key: String::new(),
            google_key: String::new(),
            parallel: 4,
        }
    }
}

fn path() -> PathBuf {
    data_root().join("prefs.json")
}

pub fn load() -> Prefs {
    workspace::maybe_json(&path()).unwrap_or_default()
}

pub fn save(p: &Prefs) -> Result<()> {
    std::fs::create_dir_all(data_root())?;
    workspace::write_json(&path(), p)
}

impl Prefs {
    pub fn key(&self) -> &str {
        match self.cloud_provider {
            Provider::Anthropic => &self.anthropic_key,
            Provider::Openai | Provider::Compatible => &self.openai_key,
            Provider::Google => &self.google_key,
        }
    }
}

/// Model files sitting next to the app, so the user picks from a list rather
/// than typing a path.
#[derive(Debug, Serialize)]
pub struct Models {
    pub speech: Vec<String>,
    pub language: Vec<String>,
}

pub fn available(dir: &std::path::Path) -> Models {
    let mut speech = Vec::new();
    let mut language = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".gguf") {
                language.push(name);
            } else if name.ends_with(".bin") && !name.contains("silero") {
                speech.push(name);
            }
        }
    }
    speech.sort();
    language.sort();
    Models { speech, language }
}
