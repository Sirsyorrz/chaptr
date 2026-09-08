import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../state/store";
import { hms, hoursMins, type TranscriptHit } from "../types";

/** What actually exists on disk per recording. Answers "did that job do anything?" */
export function Transcribes() {
  const files = useStore((s) => s.files);
  const openFile = useStore((s) => s.openFile);
  const viewing = useStore((s) => s.viewing);
  const query = useStore((s) => s.query);
  const projectId = useStore((s) => s.project?.id);
  const gotoResolve = useStore((s) => s.gotoResolve);
  const [hits, setHits] = useState<TranscriptHit[]>([]);
  const q = query.trim();

  useEffect(() => {
    if (!projectId || !q) {
      setHits([]);
      return;
    }
    let live = true;
    const t = setTimeout(async () => {
      const r = await invoke<TranscriptHit[]>("search_transcripts", { id: projectId, query: q });
      if (live) setHits(r);
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [projectId, q]);

  const done = files.filter((f) => f.transcribed).length;

  if (q) {
    let lastFile = "";
    return (
      <div className="pane">
        <div className="pane-h">
          <span>Transcript search</span>
          <span className="count">{hits.length}{hits.length >= 500 ? "+" : ""} lines</span>
        </div>
        <div className="list">
          {!hits.length && <p className="pad dim">Nothing matches.</p>}
          {hits.map((h, i) => {
            const header = h.recording_id !== lastFile;
            if (header) lastFile = h.recording_id;
            return (
              <div key={i}>
                {header && <div className="sess">{h.name}</div>}
                <div
                  className="urow"
                  title="Open this transcript and jump Resolve here"
                  onMouseDown={() => {
                    openFile(h.recording_id);
                    gotoResolve(h.recording_id, h.start);
                  }}
                >
                  <span className="uts mono">{hms(h.start)}</span>
                  <span className="uwho">{h.who === "all" ? "" : h.who}</span>
                  <span className="utext">{h.text}</span>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    );
  }

  return (
    <div className="pane">
      <div className="pane-h">
        <span>Transcribes</span>
        <span className="count">{done} / {files.length} transcribed</span>
      </div>
      <div className="list">
        {!files.length && <p className="pad dim">Scan a project first.</p>}
        {files.map((f) => (
          <div
            key={f.id}
            className={"frow" + (viewing === f.id ? " sel" : "") + (f.missing ? " offline" : "")}
            onMouseDown={() => openFile(f.id)}
          >
            <span className="fname">{f.name}</span>
            <span className="mono dim">{hoursMins(f.global_offset)}</span>
            <span className="mono dim">{hms(f.duration)}</span>
            <span className={"pill" + (f.transcribed ? " ok" : "")}>
              {f.transcribed ? `${f.segments} lines` : "not transcribed"}
            </span>
            <span className={"pill" + (f.chaptrs ? " ok" : "")}>
              {f.chaptrs ? `${f.chaptrs} chaptrs` : "—"}
            </span>
            {f.missing && (
              <span className="pill bad" title="The video file is not on this machine">
                no file
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
