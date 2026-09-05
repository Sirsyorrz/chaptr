import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../state/store";
import { hms } from "../types";

export function Transcript() {
  const current = useStore((s) => s.current);
  const time = useStore((s) => s.time);
  const setTime = useStore((s) => s.setTime);
  const addMarker = useStore((s) => s.addMarker);
  const [query, setQuery] = useState("");
  const [follow, setFollow] = useState(true);
  const listRef = useRef<HTMLDivElement>(null);
  const activeRef = useRef<HTMLDivElement>(null);

  const rows = useMemo(() => {
    const all = current?.clip.timeline ?? [];
    const q = query.trim().toLowerCase();
    return q ? all.filter((u) => u.text.toLowerCase().includes(q)) : all;
  }, [current, query]);

  const activeIndex = useMemo(() => {
    let idx = -1;
    for (let i = 0; i < rows.length; i++) {
      if (rows[i].start <= time) idx = i;
      else break;
    }
    return idx;
  }, [rows, time]);

  useEffect(() => {
    if (!follow || !activeRef.current || !listRef.current) return;
    const el = activeRef.current;
    const box = listRef.current;
    const top = el.offsetTop - box.offsetTop;
    if (top < box.scrollTop || top > box.scrollTop + box.clientHeight - 40) {
      box.scrollTo({ top: top - box.clientHeight / 2, behavior: "smooth" });
    }
  }, [activeIndex, follow]);

  return (
    <div className="pane">
      <div className="pane-h">
        <span>Transcript</span>
        <span className="count">{rows.length}</span>
        <span className="spacer" />
        <label className="inline">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
          follow
        </label>
        <input
          className="filter"
          placeholder="filter…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>

      <div className="list" ref={listRef}>
        {!rows.length && <p className="pad dim">Nothing here.</p>}
        {rows.map((u, i) => (
          <div
            key={i}
            ref={i === activeIndex ? activeRef : undefined}
            className={
              "urow" +
              (u.track !== "discord" ? " me" : "") +
              (i === activeIndex ? " active" : "")
            }
            onDoubleClick={() => addMarker(u.start, u.text.slice(0, 60))}
          >
            <span className="uts mono" onClick={() => setTime(u.start)} title="Seek here">
              {hms(u.start)}
            </span>
            <span className="uwho">{u.track === "discord" ? u.speaker : "me"}</span>
            <span className="utext">{u.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
