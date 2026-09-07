import { useUpdater } from "../state/updater";

/// Bottom-right nudge. Silent until there is something to say.
export function UpdateToast() {
  const u = useUpdater();
  if (u.phase === "idle" || u.phase === "checking") return null;

  return (
    <div className="toast">
      {u.phase === "found" && u.update && (
        <>
          <div>
            <b>chaptr {u.update.version} is available</b>
          </div>
          <div className="row">
            <button className="accent" onClick={() => u.install()}>Update</button>
            <button className="link" onClick={u.dismiss}>Later</button>
          </div>
        </>
      )}
      {u.phase === "downloading" && (
        <>
          <div>Downloading update… {u.percent ? `${u.percent}%` : ""}</div>
          <div className="bar">
            <div className="fill" style={{ width: `${u.percent}%` }} />
          </div>
        </>
      )}
      {u.phase === "ready" && (
        <>
          <div><b>Update installed</b></div>
          <div className="row">
            <button className="accent" onClick={() => u.restart()}>Restart now</button>
            <button className="link" onClick={u.dismiss}>Later</button>
          </div>
        </>
      )}
      {u.phase === "error" && (
        <>
          <div className="dim">{u.error}</div>
          <div className="row">
            <button className="link" onClick={u.dismiss}>Dismiss</button>
          </div>
        </>
      )}
    </div>
  );
}
