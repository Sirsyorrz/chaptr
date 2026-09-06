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
    /// 1 small .. 4 extreme. Lets the UI offer a ladder instead of filenames.
    pub tier: u8,
    pub tier_name: &'static str,
    /// Rough speed on a modern GPU, for setting expectations before a long run.
    pub speed: &'static str,
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
        id: "vad", file: "ggml-silero-v5.1.2.bin", kind: Kind::Support,
        name: "Voice detection",
        note: "Finds where people are actually speaking. Without it transcripts fill with repeated nonsense during silence.",
        bytes: 885_098, vram: 0, required: true, tier: 0, tier_name: "required", speed: "",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
    },

    Entry {
        id: "speech-small", file: "ggml-small.en.bin", kind: Kind::Speech,
        name: "Small", tier: 1, tier_name: "Small",
        note: "English only and noticeably rougher. Runs on a laptop with no graphics card.",
        bytes: 487_614_201, vram: 0, required: false, speed: "very fast",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
    },
    Entry {
        id: "speech-medium", file: "ggml-large-v3-turbo-q5_0.bin", kind: Kind::Speech,
        name: "Medium", tier: 2, tier_name: "Medium",
        note: "The best tested option and the default. Any language, and a hundred hours goes through in under an hour.",
        bytes: 574_041_195, vram: 2, required: true, speed: "about 130x realtime",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    },
    Entry {
        id: "speech-large", file: "ggml-large-v3-turbo.bin", kind: Kind::Speech,
        name: "Large", tier: 3, tier_name: "Large",
        note: "The same model at full precision. Measured no better than Medium on gameplay chat, but it is the safer pick for music or heavy accents.",
        bytes: 1_624_555_275, vram: 3, required: false, speed: "about 93x realtime",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
    },
    Entry {
        id: "speech-extreme", file: "ggml-large-v3.bin", kind: Kind::Speech,
        name: "Extreme", tier: 4, tier_name: "Extreme",
        note: "The full model rather than the turbo distillation. Tested on gameplay chat it recovered LESS than Medium while being slower, so only reach for it if the others are visibly struggling.",
        bytes: 3_095_033_483, vram: 5, required: false, speed: "about 72x realtime",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
    },

    Entry {
        id: "chaptr-small", file: "Qwen3-4B-Q4_K_M.gguf", kind: Kind::Language,
        name: "Small", tier: 1, tier_name: "Small",
        note: "For 8 GB graphics cards. Shorter, vaguer chaptrs that name people less often.",
        bytes: 2_497_281_312, vram: 6, required: false, speed: "fast",
        url: "https://huggingface.co/unsloth/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
    },
    Entry {
        id: "chaptr-medium", file: "Qwen3-8B-Q4_K_M.gguf", kind: Kind::Language,
        name: "Medium", tier: 2, tier_name: "Medium",
        note: "For 12 GB cards. Good, but drops players' names more often than the larger ones.",
        bytes: 5_027_784_512, vram: 10, required: false, speed: "fast",
        url: "https://huggingface.co/unsloth/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
    },
    Entry {
        id: "chaptr-large", file: "Qwen3-14B-Q4_K_M.gguf", kind: Kind::Language,
        name: "Large", tier: 3, tier_name: "Large",
        note: "The right default for a 16 GB card. Noticeably vaguer than Extreme, but dependable.",
        bytes: 9_001_753_984, vram: 14, required: true, speed: "about 3s per 10 minutes of footage",
        url: "https://huggingface.co/unsloth/Qwen3-14B-GGUF/resolve/main/Qwen3-14B-Q4_K_M.gguf",
    },
    Entry {
        id: "chaptr-extreme", file: "Qwen3-30B-A3B-Instruct-2507-Q4_K_M.gguf", kind: Kind::Language,
        name: "Extreme", tier: 4, tier_name: "Extreme",
        note: "Clearly the best in testing and, oddly, the fastest: only a fraction of it runs per word. Names people twice as often as the 14B.",
        bytes: 18_600_000_000, vram: 22, required: false, speed: "fast for its size",
        url: "https://huggingface.co/unsloth/Qwen3-30B-A3B-Instruct-2507-GGUF/resolve/main/Qwen3-30B-A3B-Instruct-2507-Q4_K_M.gguf",
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
