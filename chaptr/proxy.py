from __future__ import annotations

import json
import struct
import subprocess
import wave
from pathlib import Path

PEAK_RATE = 20  # buckets per second of audio


def build_proxy(video: Path, dest: Path, audio_stream: int | None, height: int = 360,
                fps: int = 30) -> Path:
    """Small h264 proxy for scrubbing. The source is AV1 at 60fps with PCM
    tracks, which the webview can decode but cannot seek through smoothly."""
    if dest.exists() and dest.stat().st_size > 0:
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(".part.mp4")

    cmd = ["ffmpeg", "-v", "error", "-y", "-i", str(video),
           "-map", "0:v:0", "-vf", f"scale=-2:{height},fps={fps}"]
    if audio_stream is not None:
        cmd += ["-map", f"0:{audio_stream}", "-c:a", "aac", "-b:a", "96k", "-ac", "2"]
    else:
        cmd += ["-an"]
    cmd += ["-c:v", "h264_nvenc", "-preset", "p4", "-cq", "32",
            "-movflags", "+faststart", str(tmp)]

    try:
        subprocess.run(cmd, check=True, capture_output=True)
    except subprocess.CalledProcessError:
        cmd = [c if c != "h264_nvenc" else "libx264" for c in cmd]
        cmd = [c if c != "p4" else "veryfast" for c in cmd]
        subprocess.run(cmd, check=True)
    tmp.rename(dest)
    return dest


def peaks(wav_path: Path, rate: int = PEAK_RATE) -> list[float]:
    with wave.open(str(wav_path), "rb") as wf:
        sr = wf.getframerate()
        width = wf.getsampwidth()
        channels = wf.getnchannels()
        bucket = max(1, sr // rate)
        out: list[float] = []
        while True:
            frames = wf.readframes(bucket)
            if not frames:
                break
            count = len(frames) // (width * channels)
            if not count:
                break
            samples = struct.unpack(f"<{count * channels}h", frames[: count * channels * 2])
            out.append(round(max(abs(s) for s in samples) / 32768.0, 3))
    return out


def build_peaks(wavs: dict[str, Path], dest: Path) -> dict:
    data = {role: peaks(path) for role, path in wavs.items() if path.exists()}
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps({"rate": PEAK_RATE, "tracks": data}))
    return data
