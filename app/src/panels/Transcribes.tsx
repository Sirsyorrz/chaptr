import { useStore } from "../state/store";
import { hms, hoursMins } from "../types";

/** What actually exists on disk per recording. Answers "did that job do anything?" */
export function Transcribes() {
  const files = useStore((s) => s.files);
  const openFile = useStore((s) => s.openFile);
  const viewing = useStore((s) => s.viewing);

  const done = files.filter((f) => f.transcribed).length;

  return (
    <div className="pane">
      <div className="pane-h">
        <span>Transcribes</span>
        <span className="count">{done} / {files.length} transcribed</span>
      </div>
      <div className="list">
        {!files.length && <p className="pad dim">Scan a project first.</p>}
        {files.map((f) => (
          <div
            key={f.id}
            className={"frow" + (viewing === f.id ? " sel" : "")}
            onMouseDown={() => openFile(f.id)}
          >
            <span className="fname">{f.name}</span>
            <span className="mono dim">{hoursMins(f.global_offset)}</span>
            <span className="mono dim">{hms(f.duration)}</span>
            <span className={"pill" + (f.transcribed ? " ok" : "")}>
              {f.transcribed ? `${f.segments} lines` : "not transcribed"}
            </span>
            <span className={"pill" + (f.chaptrs ? " ok" : "")}>
              {f.chaptrs ? `${f.chaptrs} chaptrs` : "—"}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
