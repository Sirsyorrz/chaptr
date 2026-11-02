import { useUpdater } from "../state/updater";

/// Inline in the status bar, matching soundbox. Shows nothing until there is
/// actually an update, so it stays out of the way.
export function Updater() {
  const u = useUpdater();
  if (u.phase === "idle" || u.phase === "checking") return null;

  return (
    <div className="updater">
      {u.phase === "found" && u.update && (
        <>
          <span>
            Version <b>{u.update.version}</b> is available.
          </span>
          <button onClick={() => void u.install()}>Update</button>
          <button onClick={u.dismiss}>Later</button>
        </>
      )}
      {u.phase === "downloading" && <span>Downloading… {u.percent ? `${u.percent}%` : ""}</span>}
      {u.phase === "ready" && (
        <>
          <span>Update installed.</span>
          <button onClick={() => void u.restart()}>Restart now</button>
        </>
      )}
      {u.phase === "error" && (
        <>
          <span className="dim">{u.error}</span>
          <button onClick={u.dismiss}>Dismiss</button>
        </>
      )}
    </div>
  );
}
