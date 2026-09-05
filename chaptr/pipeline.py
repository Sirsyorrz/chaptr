from __future__ import annotations

import hashlib
import json
import time
from pathlib import Path

from . import asr, audio, diarize, merge
from .probe import Clip


def clip_key(path: str) -> str:
    stem = Path(path).stem
    digest = hashlib.sha1(path.encode()).hexdigest()[:8]
    safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in stem)[:60]
    return f"{safe}__{digest}"


def transcribe_clip(clip: Clip, out_dir: Path, cfg: dict, force: bool = False) -> dict:
    key = clip_key(clip.path)
    dest = out_dir / "clips" / f"{key}.json"
    if dest.exists() and not force:
        return json.loads(dest.read_text())

    work = out_dir / "audio" / key
    result = {
        "clip": clip.path,
        "duration": clip.duration,
        "fps": clip.fps,
        "wall_start": clip.wall_start,
        "session_id": clip.session_id,
        "session_offset": clip.session_offset,
        "tracks": {},
    }

    for role in cfg["transcribe_roles"]:
        track = clip.track_for(role)
        if track is None:
            continue
        t0 = time.time()
        wav = audio.extract_track(Path(clip.path), track.index, work / f"{role}.wav")
        segments = asr.transcribe(wav)

        if role in cfg["diarize_roles"]:
            turns = diarize.diarize(wav, max_speakers=cfg.get("max_speakers"))
            labelled = merge.attribute(segments, turns)
            utts = merge.utterances(labelled)
        else:
            utts = merge.utterances([(w, role) for seg in segments for w in seg["words"]])

        result["tracks"][role] = {
            "utterances": utts,
            "elapsed": round(time.time() - t0, 1),
        }

    result["timeline"] = merge.combine(
        {role: data["utterances"] for role, data in result["tracks"].items()}
    )

    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(result, indent=1))
    return result


def cleanup_audio(out_dir: Path) -> None:
    work = out_dir / "audio"
    if not work.exists():
        return
    for wav in work.rglob("*.wav"):
        wav.unlink()
