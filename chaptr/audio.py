from __future__ import annotations

import subprocess
from pathlib import Path


def extract_track(video: Path, stream_index: int, dest: Path, sample_rate: int = 16000) -> Path:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists() and dest.stat().st_size > 0:
        return dest
    tmp = dest.with_suffix(".part.wav")
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-i", str(video),
         "-map", f"0:{stream_index}", "-vn", "-ac", "1", "-ar", str(sample_rate),
         "-c:a", "pcm_s16le", str(tmp)],
        check=True,
    )
    tmp.rename(dest)
    return dest


def loudness(path: Path) -> dict:
    out = subprocess.run(
        ["ffmpeg", "-hide_banner", "-i", str(path), "-af", "volumedetect", "-f", "null", "/dev/null"],
        capture_output=True, text=True,
    ).stderr
    stats = {}
    for key in ("mean_volume", "max_volume"):
        for line in out.splitlines():
            if key in line:
                stats[key] = float(line.split(":")[-1].strip().split()[0])
                break
    return stats
