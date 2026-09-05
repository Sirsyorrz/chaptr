import { useEffect, useMemo, useRef } from "react";
import { useStore } from "../state/store";
import { hms } from "../types";

/** The lines behind the selected chaptr, so a vague chaptr can be checked. */
export function Transcript() {
  const transcript = useStore((s) => s.transcript);
  const chaptrs = useStore((s) => s.chaptrs);
  const selected = useStore((s) => s.selected);
  const boxRef = useRef<HTMLDivElement>(null);
  const hitRef = useRef<HTMLDivElement>(null);

  const viewing = useStore((s) => s.viewing);
  const at = selected !== null ? chaptrs[selected]?.offset ?? null : null;
  const whole = viewing !== null && at === null;

  const rows = useMemo(() => {
    if (!transcript) return [];
    if (whole) return transcript.segments;
    if (at === null) return [];
    return transcript.segments.filter((s) => s.start > at - 90 && s.start < at + 90);
  }, [transcript, at, whole]);

  useEffect(() => {
    if (hitRef.current && boxRef.current) {
      const top = hitRef.current.offsetTop - boxRef.current.offsetTop;
      boxRef.current.scrollTop = top - boxRef.current.clientHeight / 2;
    }
  }, [rows]);

  const labelled = rows.some((r) => r.who === "host" || r.who === "friend");

  return (
    <div className="pane">
      <div className="pane-h">
        <span>{whole ? "Transcript" : "Around this chaptr"}</span>
        {labelled && <span className="count">host / friend</span>}
        <span className="spacer" />
        {transcript && <span className="dim">{whole ? `${rows.length} lines` : "±90s"}</span>}
      </div>
      <div className="list" ref={boxRef}>
        {selected === null && !viewing && (
          <p className="pad dim">Select a chaptr, or pick a file to read its transcript.</p>
        )}
        {selected !== null && !transcript && (
          <p className="pad dim">No transcript for this recording.</p>
        )}
        {rows.map((s, i) => {
          const closest = at !== null && Math.abs(s.start - at) < 3;
          return (
            <div
              key={i}
              ref={closest ? hitRef : undefined}
              className={"urow" + (closest ? " active" : "") + (s.who === "host" ? " host" : "")}
            >
              <span className="uts mono">{hms(s.start)}</span>
              <span className="uwho">{s.who === "all" ? "" : s.who}</span>
              <span className="utext">{s.text}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
