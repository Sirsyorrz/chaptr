from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

PAGE = (Path(__file__).parent / "static" / "review.html")


def marker_path(out_dir: Path, key: str) -> Path:
    return out_dir / "markers" / f"{key}.json"


def edited_path(out_dir: Path, key: str) -> Path:
    return out_dir / "markers" / f"{key}.edited.json"


def load_markers(out_dir: Path, key: str) -> dict:
    edited = edited_path(out_dir, key)
    src = edited if edited.exists() else marker_path(out_dir, key)
    if not src.exists():
        return {"markers": [], "edited": False}
    data = json.loads(src.read_text())
    data["edited"] = edited.exists()
    return data


def serve(out_dir: Path, port: int = 8756):
    keys = sorted(p.stem for p in (out_dir / "clips").glob("*.json"))

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def _send(self, code, body, ctype="application/json"):
            raw = body if isinstance(body, bytes) else body.encode()
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        def do_GET(self):
            path = urlparse(self.path).path
            if path in ("/", "/index.html"):
                return self._send(200, PAGE.read_bytes(), "text/html; charset=utf-8")
            if path == "/api/clips":
                return self._send(200, json.dumps(keys))
            if path.startswith("/api/clip/"):
                key = path.rsplit("/", 1)[-1]
                clip = out_dir / "clips" / f"{key}.json"
                if not clip.exists():
                    return self._send(404, json.dumps({"error": "unknown clip"}))
                data = json.loads(clip.read_text())
                data["markers"] = load_markers(out_dir, key)["markers"]
                return self._send(200, json.dumps(data))
            self._send(404, json.dumps({"error": "not found"}))

        def do_POST(self):
            path = urlparse(self.path).path
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            if path.startswith("/api/clip/"):
                key = path.rsplit("/", 1)[-1]
                dest = edited_path(out_dir, key)
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_text(json.dumps({"markers": body.get("markers", [])}, indent=1))
                return self._send(200, json.dumps({"saved": len(body.get("markers", []))}))
            self._send(404, json.dumps({"error": "not found"}))

    srv = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"review UI -> http://127.0.0.1:{port}  ({len(keys)} clips)")
    print("edits save to markers/<clip>.edited.json; ctrl-c to stop")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped")
