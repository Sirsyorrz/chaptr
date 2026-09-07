import { useMemo, useRef } from "react";
import { useStore } from "../state/store";
import { hms, hoursMins, type Chaptr } from "../types";

export function Chaptrs() {
  const chaptrs = useStore((s) => s.chaptrs);
  const library = useStore((s) => s.library);
  const selected = useStore((s) => s.selected);
  const query = useStore((s) => s.query);
  const starredOnly = useStore((s) => s.starredOnly);
  const select = useStore((s) => s.select);
  const star = useStore((s) => s.star);
  const remove = useStore((s) => s.remove);
  const listRef = useRef<HTMLDivElement>(null);

  const byId = useMemo(() => {
    const m = new Map<string, { name: string; session: number }>();
    library?.recordings.forEach((r) => m.set(r.id, { name: r.name, session: r.session_id }));
    return m;
  }, [library]);

  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return chaptrs
      .map((b, i) => ({ b, i }))
      .filter(({ b }) => (starredOnly ? b.starred : true))
      .filter(({ b }) => (q ? b.text.toLowerCase().includes(q) : true));
  }, [chaptrs, query, starredOnly]);

  if (!chaptrs.length) {
    return (
      <div className="pane">
        <div className="pane-h"><span>Chaptrs</span></div>
        <p className="pad dim">
          No chaptrs yet. Scan the folder, confirm the track roles, then run the
          transcribe and chaptrs passes.
        </p>
      </div>
    );
  }

  let lastRecording = "";

  return (
    <div className="pane">
      <div className="pane-h">
        <span>Chaptrs</span>
        <span className="count">{rows.length}{rows.length !== chaptrs.length ? ` / ${chaptrs.length}` : ""}</span>
        <span className="spacer" />
        <span className="dim">{chaptrs.filter((b) => b.starred).length} starred</span>
      </div>

      <div className="list" ref={listRef}>
        {rows.map(({ b, i }: { b: Chaptr; i: number }) => {
          const meta = byId.get(b.recording_id);
          // Per recording, not per session: a session is several files and the
          // swap between them is exactly what the header is there to show.
          const header = meta && b.recording_id !== lastRecording;
          if (header) lastRecording = b.recording_id;
          return (
            <div key={i}>
              {header && (
                <div className="sess">
                  session {meta!.session} · {meta!.name}
                </div>
              )}
              <div
                className={"brow" + (i === selected ? " sel" : "") + (b.starred ? " star" : "")}
                onMouseDown={() => select(i)}
              >
                <button
                  className="starbtn"
                  onClick={(e) => { e.stopPropagation(); star(i); }}
                  title="Star this chaptr"
                >
                  {b.starred ? "★" : "☆"}
                </button>
                <span className="gt mono" title="Position in the whole shoot">
                  {hoursMins(b.global)}
                </span>
                <span className="lt mono" title="Position in this file">
                  {hms(b.offset)}
                </span>
                <span className="btext" title={b.text}>{b.text}</span>
                <button className="x" onClick={() => remove(i)} title="Delete">✕</button>
              </div>
            </div>
          );
        })}
        {!rows.length && <p className="pad dim">Nothing matches.</p>}
      </div>
    </div>
  );
}
