use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub index: u32,
    pub name: String,
    pub codec: String,
    pub channels: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    pub path: String,
    pub name: String,
    pub duration: f64,
    /// Local wall-clock start, ISO 8601 to the second.
    pub wall_start: String,
    /// "filename" when parsed from the name, "mtime" when inferred.
    pub stamp_source: String,
    pub session_id: usize,
    /// Seconds from the start of this recording's session.
    pub session_offset: f64,
    /// Seconds from the start of the whole library. The number the user thinks in.
    pub global_offset: f64,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: usize,
    pub label: String,
    pub start: String,
    pub duration: f64,
    pub global_offset: f64,
    pub recordings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub root: String,
    pub scanned_at: String,
    pub total_duration: f64,
    pub sessions: Vec<Session>,
    pub recordings: Vec<Recording>,
}

impl Library {
    pub fn get(&self, id: &str) -> Option<&Recording> {
        self.recordings.iter().find(|r| r.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// "me" for the mic track, "others" for voice chat, "all" when unsplit.
    pub who: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub recording_id: String,
    pub duration: f64,
    pub model: String,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beat {
    /// Seconds from the start of the library.
    pub global: f64,
    pub recording_id: String,
    /// Seconds into that recording. What you type into the editor.
    pub offset: f64,
    pub text: String,
    pub tag: String,
    #[serde(default)]
    pub starred: bool,
    /// "llm" or "manual".
    pub source: String,
}

pub const TAGS: [&str; 8] = [
    "combat", "death", "objective", "highlight",
    "banter", "planning", "downtime", "meta",
];
