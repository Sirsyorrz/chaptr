import { useMemo, useRef } from "react";
import { useStore } from "../state/store";
import { hms, hoursMins, type Beat } from "../types";

export function Beats() {
  const beats = useStore((s) => s.beats);
  const library = useStore((s) => s.library);
  const selected = useStore((s) => s.selected);
  const query = useStore((s) => s.query);
  const starredOnly = useStore((s) => s.starredOnly);
  const select = useStore((s) => s.select);
  const star = useStore((s) => s.star);
  const edit = useStore((s) => s.edit);
  const remove = useStore((s) => s.remove);
  const listRef = useRef<HTMLDivElement>(null);

  const byId = useMemo(() => {
    const m = new Map<string, { name: string; session: number }>();
    library?.recordings.forEach((r) => m.set(r.id, { name: r.name, session: r.session_id }));
    return m;
  }, [library]);

  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return beats
      .map((b, i) => ({ b, i }))
      .filter(({ b }) => (starredOnly ? b.starred : true))
      .filter(({ b }) => (q ? b.text.toLowerCase().includes(q) : true));
  }, [beats, query, starredOnly]);

  if (!beats.length) {
    return (
      <div className="pane">
        <div className="pane-h"><span>Beats</span></div>
        <p className="pad dim">
          No beats yet. Scan the folder, confirm the track roles, then run the
          transcribe and beats passes.
        </p>
      </div>
    );
  }

  let lastSession = -1;

  return (
    <div className="pane">
      <div className="pane-h">
        <span>Beats</span>
        <span className="count">{rows.length}{rows.length !== beats.length ? ` / ${beats.length}` : ""}</span>
        <span className="spacer" />
        <span className="dim">{beats.filter((b) => b.starred).length} starred</span>
      </div>

      <div className="list" ref={listRef}>
        {rows.map(({ b, i }: { b: Beat; i: number }) => {
          const meta = byId.get(b.recording_id);
          const header = meta && meta.session !== lastSession;
          if (header) lastSession = meta!.session;
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
                  title="Star this beat"
                >
                  {b.starred ? "★" : "☆"}
                </button>
                <span className="gt mono" title="Position in the whole shoot">
                  {hoursMins(b.global)}
                </span>
                <span className="lt mono" title="Position in this file">
                  {hms(b.offset)}
                </span>
                <input
                  className="btext"
                  value={b.text}
                  onChange={(e) => edit(i, e.target.value)}
                />
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
