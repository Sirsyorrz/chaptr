from __future__ import annotations

from pathlib import Path

_model = None


def load(model_size: str = "large-v3", device: str = "cuda", compute_type: str = "float16"):
    global _model
    if _model is None:
        from faster_whisper import WhisperModel
        _model = WhisperModel(model_size, device=device, compute_type=compute_type)
    return _model


def unload():
    global _model
    _model = None
    try:
        import torch
        torch.cuda.empty_cache()
    except ImportError:
        pass


def transcribe(wav: Path, language: str = "en", beam_size: int = 5) -> list[dict]:
    model = load()
    segments, _ = model.transcribe(
        str(wav),
        language=language,
        beam_size=beam_size,
        vad_filter=True,
        word_timestamps=True,
    )
    out = []
    for seg in segments:
        out.append({
            "start": round(seg.start, 3),
            "end": round(seg.end, 3),
            "text": seg.text.strip(),
            "words": [
                {"start": round(w.start, 3), "end": round(w.end, 3), "word": w.word}
                for w in (seg.words or [])
            ],
        })
    return out
