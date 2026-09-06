import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useStore } from "./state/store";
import { Chaptrs } from "./panels/Chaptrs";
import { Transcribes } from "./panels/Transcribes";
import { Transcript } from "./panels/Transcript";
import { Tracks } from "./panels/Tracks";
import { FindChaptrs } from "./panels/FindChaptrs";
import { Settings } from "./panels/Settings";
import { hoursMins, type JobProgress } from "./types";

export default function App() {
  const [showTracks, setShowTracks] = useState(false);
  const [showFind, setShowFind] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const s = useStore();

  useEffect(() => {
    s.boot();
    const un = listen<JobProgress>("job", (e) => useStore.getState().onJob(e.payload));
    const warn = (e: BeforeUnloadEvent) => {
      if (useStore.getState().project?.unsaved) e.preventDefault();
    };
    addEventListener("beforeunload", warn);
    return () => {
      un.then((f) => f());
      removeEventListener("beforeunload", warn);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      const typing = tag === "INPUT" || tag === "TEXTAREA";
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        save();
      } else if (!typing && e.key === "/") {
        e.preventDefault();
        document.querySelector<HTMLInputElement>(".search")?.focus();
      } else if (!typing && s.selected !== null) {
        if (e.key === "j" || e.key === "ArrowDown") s.select(Math.min(s.chaptrs.length - 1, s.selected + 1));
        if (e.key === "k" || e.key === "ArrowUp") s.select(Math.max(0, s.selected - 1));
        if (e.key === " ") { e.preventDefault(); s.star(s.selected); }
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  });

  const openProjectFile = async () => {
    const f = await openDialog({
      title: "Open a chaptr project",
      filters: [{ name: "chaptr project", extensions: ["chaptr"] }],
    });
    if (typeof f === "string") s.openFile_(f);
  };

  /// One Save: keeps any chaptr edits, then writes the project file, asking
  /// for a location the first time.
  const save = async () => {
    if (s.dirty) await s.save();
    if (s.project?.file) return s.saveProject();
    const f = await saveDialog({
      title: "Save chaptr project",
      defaultPath: `${s.project?.name ?? "project"}.chaptr`,
      filters: [{ name: "chaptr project", extensions: ["chaptr"] }],
    });
    if (typeof f === "string") s.saveProject(f);
  };

  const importClips = async () => {
    const files = await openDialog({
      multiple: true,
      title: "Choose recordings",
      filters: [{ name: "Video", extensions: ["mp4", "mkv", "mov", "flv"] }],
    });
    if (Array.isArray(files) && files.length) s.openSources(files as string[]);
    else if (typeof files === "string") s.openSources([files]);
  };

  const lib = s.library;

  return (
    <div className="app">
      <div className="topbar">
        <span className="brand">chaptr</span>
        <span className="title" title={s.project?.sources.join("\n") || ""}>
          {s.project ? s.project.name : "no project"}
          {s.project?.unsaved && <b className="dot">•</b>}
        </span>
        <button onClick={importClips} title="Choose one or more recordings">
          Import
        </button>
        <button onClick={openProjectFile} title="Open a saved chaptr project">Open</button>
        {s.project && (
          <button
            onClick={() => s.confirmDiscard() && s.closeProject(false)}
            title="Close this project"
          >
            Close
          </button>
        )}
        <button onClick={() => s.runJob("transcribe")} disabled={!lib || !!s.busy}>
          Transcribe
        </button>
        <button onClick={() => setShowFind(true)} disabled={!lib || !!s.busy}>
          Find chaptrs
        </button>
        {s.busy && <button className="warn" onClick={s.cancelJob}>Stop</button>}
        <button onClick={() => setShowTracks(true)} disabled={!lib}>Tracks</button>
        <input
          className="search"
          placeholder="search chaptrs  ( / )"
          value={s.query}
          onChange={(e) => s.setQuery(e.target.value)}
        />
        <label className="inline">
          <input type="checkbox" checked={s.starredOnly} onChange={s.toggleStarredOnly} />
          starred
        </label>
        <span className="spacer" />
        <button onClick={() => setShowSettings(true)}>Settings</button>
        <button
          className={s.project?.unsaved || s.dirty ? "accent" : ""}
          onClick={save}
          disabled={!s.project || !!s.busy}
          title={s.project?.file ?? "Not saved yet"}
        >
          Save
        </button>
      </div>

      {s.missing.length > 0 && (
        <div className="banner bad">
          Missing: {s.missing.join(", ")} — put them in the app's bin/ folder.
        </div>
      )}

      {s.job && !s.job.done && (
        <div className="jobbar">
          <span className="jstage">{s.job.stage}</span>
          <span className="dim">
            {s.job.total ? `${s.job.index} / ${s.job.total}` : ""} {s.job.name}
          </span>
          <div className="bar">
            <div
              className="fill"
              style={{
                width: `${
                  s.job.total
                    ? ((s.job.index - 1 + s.job.fraction) / s.job.total) * 100
                    : s.job.fraction * 100
                }%`,
              }}
            />
          </div>
          <span className="mono dim">{(s.job.fraction * 100).toFixed(0)}%</span>
          {s.job.message && <span className="dim">{s.job.message}</span>}
        </div>
      )}

      {!s.project && (
        <div className="empty-state">
          <h2>No project open</h2>
          <p>
            Choose your recordings to start, or open a saved project.
            Select every clip in a folder if you want the whole shoot.
          </p>
          <div className="row">
            <button onClick={importClips}>Import recordings</button>
            <button onClick={openProjectFile}>Open a project</button>
          </div>
        </div>
      )}

      {s.project && (
      <div className="main">
        <div className="left">
          <div className="tabs">
            <button className={s.tab === "chaptrs" ? "on" : ""} onClick={() => s.setTab("chaptrs")}>
              Chaptrs {s.chaptrs.length ? `(${s.chaptrs.length})` : ""}
            </button>
            <button
              className={s.tab === "transcribes" ? "on" : ""}
              onClick={() => s.setTab("transcribes")}
            >
              Transcribes {s.files.length ? `(${s.files.length})` : ""}
            </button>
          </div>
          {s.tab === "chaptrs" ? <Chaptrs /> : <Transcribes />}
        </div>
        <div className="right"><Transcript /></div>
      </div>
      )}

      <div className="statusbar">
        <span>
          {lib
            ? `${lib.recordings.length} recordings · ${lib.sessions.length} sessions · ${hoursMins(lib.total_duration)}`
            : "no folder"}
        </span>
        <span className="dim">{s.chaptrs.length} chaptrs</span>
        {s.project && (
          <span className={s.project.unsaved ? "warn" : "dim"}>
            {s.project.file
              ? s.project.file.split("/").pop() + (s.project.unsaved ? " • unsaved" : "")
              : "not saved"}
          </span>
        )}
        {s.busy && <span className="warn">{s.busy}…</span>}
        <span className="spacer" />
        <span className="dim">/ search · j k move · space star · ctrl+S save</span>
        <span className={s.statusKind}>{s.status}</span>
      </div>

      {showTracks && <Tracks onClose={() => setShowTracks(false)} />}
      {showFind && <FindChaptrs onClose={() => setShowFind(false)} />}
      {showSettings && <Settings onClose={() => setShowSettings(false)} />}
    </div>
  );
}
