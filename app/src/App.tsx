import { useEffect } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useStore } from "./state/store";
import { Player } from "./panels/Player";
import { Markers } from "./panels/Markers";
import { Transcript } from "./panels/Transcript";
import { Timeline } from "./panels/Timeline";
import { hms } from "./types";

export default function App() {
  const workspace = useStore((s) => s.workspace);
  const clips = useStore((s) => s.clips);
  const current = useStore((s) => s.current);
  const dirty = useStore((s) => s.dirty);
  const status = useStore((s) => s.status);
  const statusKind = useStore((s) => s.statusKind);
  const error = useStore((s) => s.error);
  const loading = useStore((s) => s.loading);
  const markers = useStore((s) => s.markers);
  const setWorkspace = useStore((s) => s.setWorkspace);
  const refresh = useStore((s) => s.refresh);
  const openClip = useStore((s) => s.open);
  const save = useStore((s) => s.save);
  const revert = useStore((s) => s.revert);
  const setPlaying = useStore((s) => s.setPlaying);
  const playing = useStore((s) => s.playing);
  const time = useStore((s) => s.time);
  const setTime = useStore((s) => s.setTime);
  const addMarker = useStore((s) => s.addMarker);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === " ") {
        e.preventDefault();
        setPlaying(!playing);
      } else if (e.key === "m") {
        addMarker(time);
      } else if (e.key === "ArrowLeft") {
        setTime(Math.max(0, time - (e.shiftKey ? 10 : 1)));
      } else if (e.key === "ArrowRight") {
        setTime(time + (e.shiftKey ? 10 : 1));
      } else if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        save();
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, [playing, time, setPlaying, setTime, addMarker, save]);

  const pickWorkspace = async () => {
    const dir = await openDialog({ directory: true, title: "Pick a chaptr output folder" });
    if (typeof dir === "string") setWorkspace(dir);
  };

  return (
    <div className="app">
      <div className="topbar">
        <span className="brand">chaptr</span>
        <button onClick={pickWorkspace} title={workspace || "No workspace"}>
          {workspace ? workspace.split("/").pop() : "Open workspace…"}
        </button>
        <select
          value={current?.key ?? ""}
          onChange={(e) => openClip(e.target.value)}
          disabled={!clips.length}
        >
          {!clips.length && <option>no clips</option>}
          {clips.map((c) => (
            <option key={c.key} value={c.key}>
              {c.name} · {hms(c.duration)} · {c.markers}m{c.edited ? " ✎" : ""}
            </option>
          ))}
        </select>
        <span className="spacer" />
        {current?.edited && <button onClick={revert} title="Discard edits">revert</button>}
        <button className={dirty ? "accent" : ""} onClick={save} disabled={!current}>
          Save
        </button>
      </div>

      {error && (
        <div className="banner bad">
          {error}
          <button onClick={pickWorkspace}>Choose folder</button>
        </div>
      )}

      <div className="main">
        <div className="left">
          <Player />
        </div>
        <div className="right">
          <Markers />
          <Transcript />
        </div>
      </div>

      <Timeline />

      <div className="statusbar">
        <span>{current ? current.clip.clip.split("/").pop() : "—"}</span>
        <span className="dim">{markers.length} markers</span>
        {loading && <span className="dim">loading…</span>}
        <span className="spacer" />
        <span className="dim">space play · m mark · ←/→ step · ctrl+S save · ctrl+wheel zoom</span>
        <span className={statusKind}>{status}</span>
      </div>
    </div>
  );
}
