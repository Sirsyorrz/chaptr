from __future__ import annotations

import subprocess
from pathlib import Path

SAMPLE_SECONDS = 60


def _sample(video: Path, stream: int, start: float, dur: float, dest: Path) -> Path:
    dest.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-ss", str(start), "-t", str(dur), "-i", str(video),
         "-map", f"0:{stream}", "-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", str(dest)],
        check=True,
    )
    return dest


def _speech_stats(wav: Path) -> tuple[float, int]:
    """Return (speech seconds, distinct speaker count) for a sample."""
    from faster_whisper.vad import VadOptions, get_speech_timestamps
    from faster_whisper.audio import decode_audio

    pcm = decode_audio(str(wav), sampling_rate=16000)
    spans = get_speech_timestamps(pcm, VadOptions(min_silence_duration_ms=500))
    speech = sum(s["end"] - s["start"] for s in spans) / 16000.0
    return speech, len(spans)


def detect_roles(video: Path, streams: list[int], duration: float, work: Path,
                 samples: int = 3) -> dict[int, dict]:
    """Sample several windows per audio stream and measure speech content.

    Positional role guesses are unreliable across OBS profile changes; what a
    track actually contains is the only durable signal.
    """
    offsets = [duration * f for f in (0.25, 0.5, 0.75)][:samples]
    stats: dict[int, dict] = {}

    for stream in streams:
        speech = 0.0
        spans = 0
        window = min(SAMPLE_SECONDS, max(5.0, duration / 4))
        for off in offsets:
            wav = _sample(video, stream, off, window, work / f"s{stream}_{int(off)}.wav")
            sp, sc = _speech_stats(wav)
            speech += sp
            spans += sc
            wav.unlink(missing_ok=True)
        total = window * len(offsets)
        stats[stream] = {
            "speech_ratio": round(speech / total, 3) if total else 0.0,
            "spans": spans,
        }
    return stats


def assign_from_stats(stats: dict[int, dict], silent_ratio=0.01) -> dict[int, str]:
    """Map stream index -> role using measured speech content.

    Heuristic: the busiest speech track is voice. If exactly one track carries
    speech it is a mixed recording; if two, the quieter one is the local mic
    (push-to-talk, fewer spans) and the busier one is the remote group.
    """
    voiced = {s: v for s, v in stats.items() if v["speech_ratio"] > silent_ratio}
    roles = {s: "unused" for s in stats}

    if not voiced:
        return roles
    if len(voiced) == 1:
        roles[next(iter(voiced))] = "mixed"
        return roles

    ranked = sorted(voiced.items(), key=lambda kv: kv[1]["speech_ratio"], reverse=True)
    roles[ranked[0][0]] = "discord"
    roles[ranked[1][0]] = "mic"
    for s, _ in ranked[2:]:
        roles[s] = "other"
    return roles
