import { useEffect, useMemo, useRef } from "react";
import { useStore } from "../state/store";
import { hms } from "../types";

/** The lines behind the selected beat, so a vague beat can be checked. */
export function Transcript() {
  const transcript = useStore((s) => s.transcript);
  const beats = useStore((s) => s.beats);
  const selected = useStore((s) => s.selected);
  const boxRef = useRef<HTMLDivElement>(null);
  const hitRef = useRef<HTMLDivElement>(null);

  const at = selected !== null ? beats[selected]?.offset ?? null : null;

  const rows = useMemo(() => {
    if (!transcript || at === null) return [];
    return transcript.segments.filter((s) => s.start > at - 90 && s.start < at + 90);
  }, [transcript, at]);

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
        <span>Around this beat</span>
        {labelled && <span className="count">host / friend</span>}
        <span className="spacer" />
        {transcript && <span className="dim">±90s</span>}
      </div>
      <div className="list" ref={boxRef}>
        {selected === null && <p className="pad dim">Select a beat.</p>}
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
