import { useState } from "react";
import { useStore } from "../state/store";
import { hms, parseHms, type Marker } from "../types";

export function Markers() {
  const markers = useStore((s) => s.markers);
  const selected = useStore((s) => s.selected);
  const time = useStore((s) => s.time);
  const select = useStore((s) => s.select);
  const setTime = useStore((s) => s.setTime);
  const update = useStore((s) => s.updateMarker);
  const remove = useStore((s) => s.deleteMarker);
  const cycleTag = useStore((s) => s.cycleTag);
  const addMarker = useStore((s) => s.addMarker);
  const [hideFill, setHideFill] = useState(false);

  const rows = markers
    .map((m, i) => ({ m, i }))
    .filter(({ m }) => !hideFill || m.source !== "gap-fill");

  return (
    <div className="pane">
      <div className="pane-h">
        <span>Markers</span>
        <span className="count">{rows.length}</span>
        <span className="spacer" />
        <label className="inline">
          <input
            type="checkbox"
            checked={hideFill}
            onChange={(e) => setHideFill(e.target.checked)}
          />
          hide fill
        </label>
        <button onClick={() => addMarker(time)} title="Add marker at playhead">+</button>
      </div>

      <div className="list">
        {!rows.length && <p className="pad dim">No markers yet.</p>}
        {rows.map(({ m, i }: { m: Marker; i: number }) => (
          <div
            key={i}
            className={
              "mrow" +
              (i === selected ? " sel" : "") +
              (m.source === "gap-fill" ? " fill" : "")
            }
            onMouseDown={() => select(i)}
          >
            <input
              className="mt mono"
              defaultValue={hms(m.seconds)}
              key={`t${i}-${m.seconds}`}
              onDoubleClick={() => setTime(m.seconds)}
              onBlur={(e) => {
                const v = parseHms(e.target.value);
                if (v === null) e.target.value = hms(m.seconds);
                else update(i, { seconds: v });
              }}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
              title="Double-click to seek here"
            />
            <div className="mfields">
              <input
                className="mtitle"
                value={m.title}
                placeholder="what happens here"
                onChange={(e) => update(i, { title: e.target.value })}
              />
              {(m.note || i === selected) && (
                <input
                  className="mnote"
                  value={m.note}
                  placeholder="note"
                  onChange={(e) => update(i, { note: e.target.value })}
                />
              )}
            </div>
            <button className={`chip ${m.tag}`} onClick={() => cycleTag(i)} title="Cycle tag">
              {m.tag}
            </button>
            <button className="x" onClick={() => remove(i)} title="Delete">✕</button>
          </div>
        ))}
      </div>
    </div>
  );
}
