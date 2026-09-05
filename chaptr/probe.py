from __future__ import annotations

import json
import re
import subprocess
from dataclasses import dataclass, field, asdict
from datetime import datetime, timedelta
from pathlib import Path

VIDEO_EXT = {".mp4", ".mkv", ".mov"}

# OBS/GeForce filename stamps seen in the wild. Order matters: most specific first.
_STAMP_PATTERNS = [
    (r"(\d{4})[-.](\d{2})[-.](\d{2})[ _-]+(\d{2})[-.](\d{2})[-.](\d{2})", "ymd"),
    (r"(\d{4})[-.](\d{2})[-.](\d{2})[ _-]+(\d{2})[-.](\d{2})()", "ymd"),
    (r"(\d{2})\.(\d{2})\.(\d{4})[ _-]+(\d{2})[-.](\d{2})[-.](\d{2})", "dmy"),
    (r"(\d{2})\.(\d{2})\.(\d{4})[ _-]+(\d{2})[-.](\d{2})()", "dmy"),
]


def parse_stamp(name: str) -> datetime | None:
    for pat, order in _STAMP_PATTERNS:
        m = re.search(pat, name)
        if not m:
            continue
        g = [x for x in m.groups()]
        sec = int(g[5]) if g[5] else 0
        try:
            if order == "ymd":
                return datetime(int(g[0]), int(g[1]), int(g[2]), int(g[3]), int(g[4]), sec)
            return datetime(int(g[2]), int(g[1]), int(g[0]), int(g[3]), int(g[4]), sec)
        except ValueError:
            continue
    return None


@dataclass
class Track:
    index: int
    name: str
    codec: str
    channels: int
    role: str = "unknown"


@dataclass
class Clip:
    path: str
    duration: float
    fps: float
    video_codec: str
    tracks: list[Track]
    wall_start: str
    stamp_source: str
    session_id: int = -1
    session_offset: float = 0.0

    @property
    def start_dt(self) -> datetime:
        return datetime.fromisoformat(self.wall_start)

    def track_for(self, role: str) -> Track | None:
        return next((t for t in self.tracks if t.role == role), None)


def _ffprobe(path: Path) -> dict:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-print_format", "json",
         "-show_format", "-show_streams", str(path)],
        capture_output=True, text=True, check=True,
    )
    return json.loads(out.stdout)


def _fps(stream: dict) -> float:
    for key in ("avg_frame_rate", "r_frame_rate"):
        val = stream.get(key, "0/0")
        num, _, den = val.partition("/")
        try:
            if float(den):
                return float(num) / float(den)
        except (TypeError, ValueError):
            pass
    return 0.0


def probe_clip(path: Path, roles: dict) -> Clip:
    data = _ffprobe(path)
    streams = data["streams"]
    video = next((s for s in streams if s["codec_type"] == "video"), {})
    audio = [s for s in streams if s["codec_type"] == "audio"]

    tracks = [
        Track(
            index=s["index"],
            name=s.get("tags", {}).get("name", f"stream{s['index']}"),
            codec=s.get("codec_name", "?"),
            channels=int(s.get("channels", 0)),
        )
        for s in audio
    ]
    assign_roles(tracks, roles)

    stamp = parse_stamp(path.name)
    duration = float(data["format"]["duration"])
    if stamp:
        source = "filename"
    else:
        # OBS writes mtime at the end of recording, so start is mtime minus runtime.
        stamp = datetime.fromtimestamp(path.stat().st_mtime) - timedelta(seconds=duration)
        source = "mtime"

    return Clip(
        path=str(path),
        duration=duration,
        fps=_fps(video),
        video_codec=video.get("codec_name", "?"),
        tracks=tracks,
        wall_start=stamp.isoformat(timespec="seconds"),
        stamp_source=source,
    )


def assign_roles(tracks: list[Track], roles: dict) -> None:
    by_name = {k.lower(): v for k, v in roles.get("by_name", {}).items()}
    for t in tracks:
        if t.name.lower() in by_name:
            t.role = by_name[t.name.lower()]

    if all(t.role == "unknown" for t in tracks):
        layout = roles.get("by_count", {}).get(str(len(tracks)))
        if layout:
            for pos, role in enumerate(layout):
                if pos < len(tracks):
                    tracks[pos].role = role


def segment_sessions(clips: list[Clip], gap_minutes: float) -> list[dict]:
    clips.sort(key=lambda c: c.start_dt)
    sessions: list[dict] = []
    gap = timedelta(minutes=gap_minutes)

    for clip in clips:
        new = True
        if sessions:
            cur = sessions[-1]
            prev_end = cur["_end"]
            same_day = clip.start_dt.date() == prev_end.date()
            if same_day and clip.start_dt - prev_end <= gap:
                new = False
        if new:
            sessions.append({
                "id": len(sessions),
                "label": clip.start_dt.strftime("%Y-%m-%d %H:%M"),
                "start": clip.start_dt,
                "_end": clip.start_dt,
                "clips": [],
            })
        s = sessions[-1]
        clip.session_id = s["id"]
        clip.session_offset = (clip.start_dt - s["start"]).total_seconds()
        s["clips"].append(clip)
        s["_end"] = clip.start_dt + timedelta(seconds=clip.duration)

    for s in sessions:
        s["duration"] = (s["_end"] - s["start"]).total_seconds()
        s["start"] = s["start"].isoformat(timespec="seconds")
        del s["_end"]
    return sessions


def build_manifest(root: Path, roles: dict, gap_minutes: float, recurse: bool) -> dict:
    it = root.rglob("*") if recurse else root.glob("*")
    paths = sorted(p for p in it if p.suffix.lower() in VIDEO_EXT and p.is_file())
    clips = [probe_clip(p, roles) for p in paths]
    sessions = segment_sessions(clips, gap_minutes)
    return {
        "root": str(root),
        "sessions": [
            {**{k: v for k, v in s.items() if k != "clips"},
             "clips": [asdict(c) for c in s["clips"]]}
            for s in sessions
        ],
    }
