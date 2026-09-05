from __future__ import annotations

import json
import re
from datetime import timedelta

import urllib.request

SCHEMA = {
    "type": "object",
    "properties": {
        "markers": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "t": {"type": "string"},
                    "title": {"type": "string"},
                    "note": {"type": "string"},
                    "tag": {
                        "type": "string",
                        "enum": ["combat", "objective", "banter", "planning",
                                 "highlight", "death", "downtime", "meta"],
                    },
                    "speakers": {"type": "array", "items": {"type": "string"}},
                },
                "required": ["t", "title", "tag"],
            },
        }
    },
    "required": ["markers"],
}

SYSTEM = """You index recorded Deadlock gameplay so a video editor can scrub straight to a moment.

Input is a timestamped transcript of voice chat between friends playing together. "me" is the person whose footage this is; other names are their friends.

Emit a marker wherever something happens that an editor would want to find again: a fight starting or ending, an objective, a death, a big play, a plan being made or abandoned, a memorable joke or reaction, a complaint that characterises the match.

Density: roughly one marker per 30 to 90 seconds of talk. Do not summarise the whole clip in two markers. Do not mark every line either.

Title style. Write what actually happened, concretely, using the names and heroes mentioned. Under 60 characters, no trailing period.
  good: "Team wipes enemy in mid, calls for push"
  good: "Warden survives on shields, me raging about it"
  good: "Josh sleeps two in the corridor"
  bad:  "Player hit by enemy"        (vague, no information)
  bad:  "Combat happens"             (says nothing)
  bad:  "Discussion about the game"  (useless to an editor)

Other rules:
- "t" must be copied exactly from a [HH:MM:SS] timestamp present in the transcript. Never invent one.
- "note" is one extra sentence, only when the title genuinely needs context. Otherwise omit it.
- Warden, Abrams, Paradox, Holiday, Mina, Seven, Lash, Vindicta and similar are game heroes, not the people talking. Never treat a hero name as a speaker.
- Choose the tag that matches the dominant activity, not the mood.
- Some speakers appear as SPEAKER_00, SPEAKER_01 and so on because they are not yet identified. Never print such a label in a title or note. Say "a friend", "someone", or describe them by what they do instead.
- Plain description. No hype, no editorialising, no "epic" or "insane"."""


_SPEAKER_LABEL = re.compile(r"\bSPEAKER[_ ]?\d+\b", re.I)


def strip_labels(text: str, names: dict[str, str]) -> str:
    """Replace raw diarization labels the model echoed back into its output."""
    def sub(m):
        return names.get(m.group(0), "a friend")
    return _SPEAKER_LABEL.sub(sub, text)


def hms(seconds: float) -> str:
    return str(timedelta(seconds=int(seconds)))


def to_seconds(stamp: str) -> float | None:
    parts = stamp.strip().strip("[]").split(":")
    try:
        parts = [float(p) for p in parts]
    except ValueError:
        return None
    while len(parts) < 3:
        parts.insert(0, 0.0)
    return parts[0] * 3600 + parts[1] * 60 + parts[2]


def render_window(utterances, speaker_names: dict[str, str]) -> str:
    lines = []
    for u in utterances:
        who = u["speaker"] if u["track"] == "discord" else "me"
        who = speaker_names.get(who, who)
        lines.append(f"[{hms(u['start'])}] {who}: {u['text']}")
    return "\n".join(lines)


def windows(utterances, window_s: float, overlap_s: float):
    if not utterances:
        return
    end = utterances[-1]["end"]
    start = utterances[0]["start"]
    while start < end:
        stop = start + window_s
        chunk = [u for u in utterances if start <= u["start"] < stop]
        if chunk:
            yield start, stop, chunk
        start = stop - overlap_s


def call_ollama(model: str, system: str, user: str, host: str, timeout: int = 900) -> dict:
    payload = {
        "model": model,
        "system": system,
        "prompt": user,
        "format": SCHEMA,
        "stream": False,
        "think": False,
        "options": {"temperature": 0.3, "num_ctx": 16384},
    }
    req = urllib.request.Request(
        f"{host}/api/generate",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        body = json.loads(resp.read())
    return json.loads(body["response"])


def enforce_spacing(markers, min_gap, max_gap, utterances):
    """Drop markers that crowd each other, then fill long silences.

    An unconstrained model clusters markers around dense audio and leaves
    quiet stretches entirely unindexed, which is exactly backwards for
    someone scrubbing footage.
    """
    markers = sorted(markers, key=lambda m: m["seconds"])
    kept: list[dict] = []
    for m in markers:
        if kept and m["seconds"] - kept[-1]["seconds"] < min_gap:
            continue
        kept.append(m)

    if not max_gap or not utterances:
        return kept

    filled: list[dict] = []
    prev = utterances[0]["start"]
    for m in kept + [None]:
        edge = m["seconds"] if m else utterances[-1]["end"]
        while edge - prev > max_gap:
            prev += max_gap
            near = min(utterances, key=lambda u: abs(u["start"] - prev))
            filled.append({
                "seconds": round(near["start"], 2),
                "t": hms(near["start"]),
                "title": near["text"][:60],
                "note": "",
                "tag": "downtime",
                "speakers": [],
                "source": "gap-fill",
                "confidence": 0.2,
            })
        if m:
            filled.append(m)
            prev = m["seconds"]
    return sorted(filled, key=lambda m: m["seconds"])


def generate(utterances, cfg, progress=None) -> list[dict]:
    model = cfg.get("model", "qwen3:30b-a3b")
    host = cfg.get("ollama_host", "http://localhost:11434")
    names = cfg.get("speakers", {})
    win = cfg.get("window_minutes", 10) * 60
    overlap = cfg.get("overlap_minutes", 1) * 60

    chunks = list(windows(utterances, win, overlap))
    out: list[dict] = []
    for i, (start, stop, chunk) in enumerate(chunks, 1):
        if progress:
            progress(i, len(chunks), start, stop)
        text = render_window(chunk, names)
        try:
            result = call_ollama(model, SYSTEM, text, host)
        except Exception as exc:
            if progress:
                progress(i, len(chunks), start, stop, error=repr(exc))
            continue
        for m in result.get("markers", []):
            secs = to_seconds(m.get("t", ""))
            if secs is None or not (start - 1 <= secs <= stop + 1):
                continue  # model invented a timestamp outside the window
            out.append({
                "seconds": round(secs, 2),
                "t": hms(secs),
                "title": strip_labels((m.get("title") or "").strip(), names)[:120],
                "note": strip_labels((m.get("note") or "").strip(), names),
                "tag": m.get("tag", "meta"),
                "speakers": m.get("speakers", []),
                "source": "llm",
                "confidence": 0.7,
            })

    deduped: list[dict] = []
    for m in sorted(out, key=lambda x: x["seconds"]):
        if deduped and abs(m["seconds"] - deduped[-1]["seconds"]) < 15 \
                and m["title"][:25].lower() == deduped[-1]["title"][:25].lower():
            continue  # same event seen twice across the window overlap
        deduped.append(m)

    return enforce_spacing(
        deduped,
        cfg.get("min_gap_seconds", 45),
        cfg.get("max_gap_seconds", 300),
        utterances,
    )
