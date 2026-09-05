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
        entry = {
            "speech_ratio": round(speech / total, 3) if total else 0.0,
            "spans": spans,
            "speakers": 0,
        }
        if entry["speech_ratio"] > 0.02:
            entry["speakers"] = _count_speakers(video, stream, offsets[0], window, work)
        stats[stream] = entry
    return stats


def _count_speakers(video: Path, stream: int, start: float, dur: float, work: Path) -> int:
    from . import diarize

    wav = _sample(video, stream, start, dur, work / f"spk{stream}.wav")
    try:
        turns = diarize.diarize(wav, max_speakers=8)
    finally:
        wav.unlink(missing_ok=True)
    # ignore blips; crosstalk produces sub-second phantom turns
    return len({t["speaker"] for t in turns if t["end"] - t["start"] >= 0.5})


def assign_from_stats(stats: dict[int, dict], silent_ratio=0.02) -> dict[int, str]:
    """Map stream index -> role using speech presence and distinct voice count.

    Speech volume alone cannot separate a mixed track from the group track,
    since the mixed track contains the group. Voice count does: the mic carries
    exactly one speaker, and the mixed track carries the most.
    """
    roles = {s: "game" for s in stats}
    voiced = {s: v for s, v in stats.items() if v["speech_ratio"] > silent_ratio}
    if not voiced:
        return roles

    solo = [s for s, v in voiced.items() if v.get("speakers", 0) == 1]
    multi = sorted(
        (s for s, v in voiced.items() if v.get("speakers", 0) > 1),
        key=lambda s: (voiced[s]["speakers"], voiced[s]["speech_ratio"]),
        reverse=True,
    )

    if len(voiced) == 1:
        roles[next(iter(voiced))] = "mixed"
        return roles

    if solo:
        roles[solo[0]] = "mic"
    if multi:
        roles[multi[0]] = "mixed" if len(multi) > 1 else "discord"
    if len(multi) > 1:
        roles[multi[1]] = "discord"
    return roles
