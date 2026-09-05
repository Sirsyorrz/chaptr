from __future__ import annotations

import os
from pathlib import Path

MODEL = "pyannote/speaker-diarization-3.1"
_pipeline = None


def load():
    global _pipeline
    if _pipeline is None:
        import torch
        from pyannote.audio import Pipeline
        token = os.environ.get("HF_TOKEN")
        if not token:
            raise RuntimeError("HF_TOKEN not set; required for gated pyannote models")
        _pipeline = Pipeline.from_pretrained(MODEL, token=token)
        if torch.cuda.is_available():
            _pipeline.to(torch.device("cuda"))
    return _pipeline


def unload():
    global _pipeline
    _pipeline = None
    try:
        import torch
        torch.cuda.empty_cache()
    except ImportError:
        pass


def diarize(wav: Path, min_speakers: int | None = None, max_speakers: int | None = None) -> list[dict]:
    pipe = load()
    kwargs = {}
    if min_speakers:
        kwargs["min_speakers"] = min_speakers
    if max_speakers:
        kwargs["max_speakers"] = max_speakers
    result = pipe(str(wav), **kwargs)
    annotation = getattr(result, "speaker_diarization", result)
    return [
        {"start": round(turn.start, 3), "end": round(turn.end, 3), "speaker": spk}
        for turn, _, spk in annotation.itertracks(yield_label=True)
    ]
