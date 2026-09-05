import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useStore } from "./state/store";
import { Beats } from "./panels/Beats";
import { Transcript } from "./panels/Transcript";
import { Tracks } from "./panels/Tracks";
import { hoursMins } from "./types";

export default function App() {
  const [showTracks, setShowTracks] = useState(false);
  const s = useStore();

  useEffect(() => {
    s.boot();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      const typing = tag === "INPUT" || tag === "TEXTAREA";
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        s.save();
      } else if (!typing && e.key === "/") {
        e.preventDefault();
        document.querySelector<HTMLInputElement>(".search")?.focus();
      } else if (!typing && s.selected !== null) {
        if (e.key === "j" || e.key === "ArrowDown") s.select(Math.min(s.beats.length - 1, s.selected + 1));
        if (e.key === "k" || e.key === "ArrowUp") s.select(Math.max(0, s.selected - 1));
        if (e.key === " ") { e.preventDefault(); s.star(s.selected); }
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  });

  const pick = async () => {
    const dir = await openDialog({ directory: true, title: "Pick a folder of recordings" });
    if (typeof dir === "string") s.openFolder(dir);
  };

  const lib = s.library;

  return (
    <div className="app">
      <div className="topbar">
        <span className="brand">chaptr</span>
        <button onClick={pick} title={s.folder || "No folder"}>
          {s.folder ? s.folder.split("/").filter(Boolean).pop() : "Open folder…"}
        </button>
        <button onClick={s.rescan} disabled={!s.folder || !!s.busy}>Scan</button>
        <button onClick={() => setShowTracks(true)} disabled={!lib}>Tracks…</button>
        <input
          className="search"
          placeholder="search beats  ( / )"
          value={s.query}
          onChange={(e) => s.setQuery(e.target.value)}
        />
        <label className="inline">
          <input type="checkbox" checked={s.starredOnly} onChange={s.toggleStarredOnly} />
          starred
        </label>
        <span className="spacer" />
        <button className={s.dirty ? "accent" : ""} onClick={s.save} disabled={!s.dirty}>
          Save
        </button>
      </div>

      {s.missing.length > 0 && (
        <div className="banner bad">
          Missing: {s.missing.join(", ")} — put them in the app's bin/ folder.
        </div>
      )}

      <div className="main">
        <div className="left"><Beats /></div>
        <div className="right"><Transcript /></div>
      </div>

      <div className="statusbar">
        <span>
          {lib
            ? `${lib.recordings.length} recordings · ${lib.sessions.length} sessions · ${hoursMins(lib.total_duration)}`
            : "no folder"}
        </span>
        <span className="dim">{s.beats.length} beats</span>
        {s.busy && <span className="warn">{s.busy}…</span>}
        <span className="spacer" />
        <span className="dim">/ search · j k move · space star · ctrl+S save</span>
        <span className={s.statusKind}>{s.status}</span>
      </div>

      {showTracks && <Tracks onClose={() => setShowTracks(false)} />}
    </div>
  );
}
