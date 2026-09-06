use serde::Serialize;

/// A deliberately short, curated list. Nobody should have to know what a
/// quantisation is — they pick "recommended" or "light" and get on with it.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub id: &'static str,
    pub file: &'static str,
    pub kind: Kind,
    pub name: &'static str,
    pub note: &'static str,
    /// Approximate; the real size comes from the server when downloading.
    pub bytes: u64,
    /// Minimum sensible VRAM in GB. 0 runs on a CPU.
    pub vram: u32,
    pub url: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Speech,
    Language,
    Support,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        id: "vad",
        file: "ggml-silero-v5.1.2.bin",
        kind: Kind::Support,
        name: "Voice detection",
        note: "Finds where people are actually speaking. Without it transcripts fill with repeated nonsense during silence.",
        bytes: 885_098,
        vram: 0,
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
        required: true,
    },
    Entry {
        id: "whisper-turbo",
        file: "ggml-large-v3-turbo-q5_0.bin",
        kind: Kind::Speech,
        name: "Transcription — recommended",
        note: "Accurate and fast: about 130x realtime on a modern graphics card, so a hundred hours takes under an hour.",
        bytes: 574_041_195,
        vram: 2,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        required: true,
    },
    Entry {
        id: "whisper-small",
        file: "ggml-small.en.bin",
        kind: Kind::Speech,
        name: "Transcription — small",
        note: "English only and rougher, but runs on a laptop with no graphics card.",
        bytes: 487_614_201,
        vram: 0,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        required: false,
    },
    Entry {
        id: "qwen4b",
        file: "Qwen3-4B-Q4_K_M.gguf",
        kind: Kind::Language,
        name: "Chaptrs — light",
        note: "For 8 GB graphics cards. Shorter, vaguer chaptrs and names people less often.",
        bytes: 2_497_281_312,
        vram: 6,
        url: "https://huggingface.co/unsloth/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
        required: false,
    },
    Entry {
        id: "qwen8b",
        file: "Qwen3-8B-Q4_K_M.gguf",
        kind: Kind::Language,
        name: "Chaptrs — balanced",
        note: "For 12 GB cards. Good, but drops players' names more often than the larger one.",
        bytes: 5_027_784_512,
        vram: 10,
        url: "https://huggingface.co/unsloth/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
        required: false,
    },
    Entry {
        id: "qwen14b",
        file: "Qwen3-14B-Q4_K_M.gguf",
        kind: Kind::Language,
        name: "Chaptrs — recommended",
        note: "For 16 GB cards and up. Holds on to players' names, which is most of what makes a chaptr worth reading.",
        bytes: 9_001_753_984,
        vram: 14,
        url: "https://huggingface.co/unsloth/Qwen3-14B-GGUF/resolve/main/Qwen3-14B-Q4_K_M.gguf",
        required: true,
    },
];

pub fn find(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.id == id)
}

/// Total VRAM in GB, used to recommend a model. Absent means we do not know,
/// and the UI should not pretend otherwise.
pub fn vram_gb() -> Option<u32> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mb: u32 = text.lines().next()?.trim().parse().ok()?;
    Some(mb / 1024)
}
