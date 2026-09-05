from __future__ import annotations

import argparse
import json
import sys
from datetime import timedelta
from pathlib import Path

from . import config, detect, pipeline
from .audio import extract_track, loudness
from .probe import Clip, Track, build_manifest


def hms(seconds: float) -> str:
    return str(timedelta(seconds=int(seconds)))


def _clip_from_dict(d: dict) -> Clip:
    d = dict(d)
    d["tracks"] = [Track(**t) for t in d["tracks"]]
    return Clip(**d)


def load_manifest(out_dir: Path) -> dict:
    path = out_dir / "manifest.json"
    if not path.exists():
        sys.exit(f"no manifest at {path} — run `scan` first")
    return json.loads(path.read_text())


def cmd_scan(args, cfg):
    root = Path(args.source).expanduser()
    manifest = build_manifest(root, cfg["roles"], cfg["session_gap_minutes"], cfg["recurse"])

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=1))

    total = 0.0
    for s in manifest["sessions"]:
        total += s["duration"]
        print(f"\nsession {s['id']:>3}  {s['label']}  {hms(s['duration'])}  "
              f"({len(s['clips'])} clips)")
        for c in s["clips"]:
            roles = ",".join(f"{t['role']}:{t['index']}" for t in c["tracks"])
            missing = "" if any(t["role"] == "discord" for t in c["tracks"]) else "  [no discord]"
            print(f"    +{hms(c['session_offset']):>9}  {hms(c['duration']):>9}  "
                  f"{c['fps']:.2f}fps  {Path(c['path']).name}")
            print(f"    {'':>9}   {roles}  via {c['stamp_source']}{missing}")

    print(f"\n{len(manifest['sessions'])} sessions, {hms(total)} total")
    print(f"manifest -> {out_dir / 'manifest.json'}")


def cmd_tracks(args, cfg):
    manifest = load_manifest(Path(args.out))
    clips = [c for s in manifest["sessions"] for c in s["clips"]]
    if args.clip:
        clips = [c for c in clips if args.clip in c["path"]]
    tmp = Path(args.out) / "audio" / "_probe"

    for c in clips[: args.limit]:
        print(f"\n{Path(c['path']).name}")
        for t in c["tracks"]:
            wav = extract_track(Path(c["path"]), t["index"], tmp / f"{t['index']}.wav")
            lvl = loudness(wav)
            wav.unlink(missing_ok=True)
            print(f"  stream {t['index']}  {t['name']:<16} role={t['role']:<8} "
                  f"mean={lvl.get('mean_volume', 0):>6.1f}dB max={lvl.get('max_volume', 0):>6.1f}dB")


def cmd_detect(args, cfg):
    out_dir = Path(args.out)
    manifest = load_manifest(out_dir)
    clips = [c for s in manifest["sessions"] for c in s["clips"]]
    if args.clip:
        clips = [c for c in clips if args.clip in c["path"]]
    if args.limit:
        clips = clips[: args.limit]

    work = out_dir / "audio" / "_detect"
    for c in clips:
        streams = [t["index"] for t in c["tracks"]]
        stats = detect.detect_roles(Path(c["path"]), streams, c["duration"], work)
        roles = detect.assign_from_stats(stats)
        print(f"\n{Path(c['path']).name}")
        for t in c["tracks"]:
            st = stats[t["index"]]
            was, now = t["role"], roles[t["index"]]
            flag = "" if was == now else f"  (was {was})"
            print(f"  stream {t['index']}  speech={st['speech_ratio']:>5.1%} "
                  f"spk={st.get('speakers',0)}  role={now}{flag}")
            t["role"] = now

    if args.write:
        (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=1))
        print("\nmanifest updated")


def cmd_transcribe(args, cfg):
    out_dir = Path(args.out)
    manifest = load_manifest(out_dir)
    clips = [_clip_from_dict(c) for s in manifest["sessions"] for c in s["clips"]]
    if args.session is not None:
        clips = [c for c in clips if c.session_id == args.session]
    if args.clip:
        clips = [c for c in clips if args.clip in c.path]
    if args.limit:
        clips = clips[: args.limit]

    if not clips:
        sys.exit("no clips matched")

    print(f"{len(clips)} clips, {hms(sum(c.duration for c in clips))} of footage")
    failures = []
    for i, clip in enumerate(clips, 1):
        name = Path(clip.path).name
        print(f"[{i}/{len(clips)}] {name}", flush=True)
        try:
            res = pipeline.transcribe_clip(clip, out_dir, cfg, force=args.force)
            for role, data in res["tracks"].items():
                print(f"    {role:<8} {len(data['utterances']):>4} utterances  {data['elapsed']}s")
        except Exception as exc:  # keep the batch alive; report at the end
            failures.append((name, repr(exc)))
            print(f"    FAILED {exc!r}")

    if not args.keep_audio:
        pipeline.cleanup_audio(out_dir)
    if failures:
        print(f"\n{len(failures)} failures:")
        for name, err in failures:
            print(f"  {name}: {err}")


def cmd_show(args, cfg):
    out_dir = Path(args.out)
    path = out_dir / "clips" / f"{pipeline.clip_key(args.clip)}.json"
    if not path.exists():
        matches = list((out_dir / "clips").glob("*.json"))
        matches = [m for m in matches if args.clip in m.stem]
        if not matches:
            sys.exit("no transcript found for that clip")
        path = matches[0]
    data = json.loads(path.read_text())
    for u in data["timeline"]:
        who = u["speaker"] if u["track"] == "discord" else u["track"]
        print(f"[{hms(u['start'])}] {who:<12} {u['text']}")


def main(argv=None):
    ap = argparse.ArgumentParser(prog="chaptr")
    ap.add_argument("--config", type=Path)
    ap.add_argument("-o", "--out", default="./chaptr-out")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("scan", help="probe footage, build manifest, segment sessions")
    p.add_argument("source")
    p.set_defaults(fn=cmd_scan)

    p = sub.add_parser("tracks", help="report audio levels per track to verify role mapping")
    p.add_argument("--clip")
    p.add_argument("--limit", type=int, default=3)
    p.set_defaults(fn=cmd_tracks)

    p = sub.add_parser("detect", help="infer track roles from measured speech content")
    p.add_argument("--clip")
    p.add_argument("--limit", type=int, default=5)
    p.add_argument("--write", action="store_true", help="persist detected roles to manifest")
    p.set_defaults(fn=cmd_detect)

    p = sub.add_parser("transcribe", help="whisper + diarize every clip")
    p.add_argument("--session", type=int)
    p.add_argument("--clip")
    p.add_argument("--limit", type=int)
    p.add_argument("--force", action="store_true")
    p.add_argument("--keep-audio", action="store_true")
    p.set_defaults(fn=cmd_transcribe)

    p = sub.add_parser("show", help="print a clip transcript")
    p.add_argument("clip")
    p.set_defaults(fn=cmd_show)

    args = ap.parse_args(argv)
    cfg = config.load(args.config)
    args.fn(args, cfg)


if __name__ == "__main__":
    main()
