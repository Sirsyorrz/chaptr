from __future__ import annotations

import tomllib
from pathlib import Path

DEFAULTS = {
    "session_gap_minutes": 45.0,
    "recurse": True,
    "speakers": {},
    "roles": {
        "by_name": {
            "microphone": "mic",
            "system sounds": "game",
            "mic": "mic",
            "discord": "discord",
        },
        # positional layout keyed by audio track count; 4-track is the OBS
        # profile used for new recordings.
        "by_count": {
            "2": ["mixed", "mic"],
            "3": ["mixed", "mic", "game"],
            "4": ["mixed", "mic", "game", "discord"],
        },
    },
    "transcribe_roles": ["mic", "discord"],
    "diarize_roles": ["discord"],
    "max_speakers": 6,
}


def _deep_merge(base: dict, over: dict) -> dict:
    out = dict(base)
    for k, v in over.items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _deep_merge(out[k], v)
        else:
            out[k] = v
    return out


def load(path: Path | None) -> dict:
    if path is None:
        for cand in (Path("chaptr.toml"), Path.home() / ".config/chaptr/config.toml"):
            if cand.exists():
                path = cand
                break
    if path is None or not path.exists():
        return dict(DEFAULTS)
    with open(path, "rb") as fh:
        return _deep_merge(DEFAULTS, tomllib.load(fh))
