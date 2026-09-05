from __future__ import annotations


def _speaker_at(turns, start, end, default="?"):
    best, overlap = default, 0.0
    for t in turns:
        ov = min(end, t["end"]) - max(start, t["start"])
        if ov > overlap:
            best, overlap = t["speaker"], ov
    return best


def attribute(segments, turns, min_turn=0.4, min_words=3):
    """Label every ASR word with a diarization speaker, then smooth the label run.

    pyannote emits sub-second turns during crosstalk which otherwise flip the
    speaker mid-clause and shred utterances into single words.
    """
    turns = [t for t in turns if t["end"] - t["start"] >= min_turn]
    words = [w for seg in segments for w in seg["words"]]
    if not words:
        return []

    labels = [_speaker_at(turns, w["start"], w["end"]) for w in words]

    for i, lab in enumerate(labels):
        if lab == "?":
            labels[i] = labels[i - 1] if i else next((x for x in labels if x != "?"), "?")

    runs = []
    for i, lab in enumerate(labels):
        if runs and runs[-1][0] == lab:
            runs[-1][2] = i
        else:
            runs.append([lab, i, i])

    changed = True
    while changed and len(runs) > 1:
        changed = False
        for idx, (lab, lo, hi) in enumerate(runs):
            if hi - lo + 1 >= min_words:
                continue
            prev = runs[idx - 1] if idx > 0 else None
            nxt = runs[idx + 1] if idx + 1 < len(runs) else None
            if prev and nxt and prev[0] == nxt[0]:
                target = prev[0]
            elif prev and (not nxt or (prev[2] - prev[1]) >= (nxt[2] - nxt[1])):
                target = prev[0]
            elif nxt:
                target = nxt[0]
            else:
                continue
            if target != lab:
                for j in range(lo, hi + 1):
                    labels[j] = target
                changed = True
                break
        if changed:
            runs = []
            for i, lab in enumerate(labels):
                if runs and runs[-1][0] == lab:
                    runs[-1][2] = i
                else:
                    runs.append([lab, i, i])

    return list(zip(words, labels))


def utterances(labelled, max_gap=1.2):
    out = []
    for word, spk in labelled:
        if out and out[-1]["speaker"] == spk and word["start"] - out[-1]["end"] <= max_gap:
            out[-1]["end"] = word["end"]
            out[-1]["text"] += word["word"]
        else:
            out.append({
                "start": word["start"],
                "end": word["end"],
                "speaker": spk,
                "text": word["word"],
            })
    for u in out:
        u["text"] = u["text"].strip()
        u["start"] = round(u["start"], 2)
        u["end"] = round(u["end"], 2)
    return [u for u in out if u["text"]]


def combine(tracks: dict[str, list[dict]]) -> list[dict]:
    """Interleave utterance lists from multiple audio roles into one timeline."""
    merged = []
    for role, utts in tracks.items():
        for u in utts:
            merged.append({**u, "track": role})
    merged.sort(key=lambda u: u["start"])
    return merged
