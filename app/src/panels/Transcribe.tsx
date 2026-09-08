import { useState } from "react";
import { useStore } from "../state/store";
import { Vocabulary } from "./Vocabulary";

export function Transcribe({ onClose }: { onClose: () => void }) {
  const settings = useStore((s) => s.settings);
  const saveSettings = useStore((s) => s.saveSettings);
  const runJob = useStore((s) => s.runJob);
  const files = useStore((s) => s.files);

  const [notes, setNotes] = useState(settings?.notes ?? "");

  const go = async () => {
    await saveSettings({ notes });
    onClose();
    runJob("transcribe");
  };

  return (
    <div className="sheet" onMouseDown={onClose}>
      <div className="sheet-body narrow" onMouseDown={(e) => e.stopPropagation()}>
        <div className="pane-h">
          <span>Transcribe</span>
          <span className="spacer" />
          <span className="dim">{files.length} files</span>
        </div>

        <div className="sheet-scroll">
          <p className="note">
            chaptr listens to every voice track and writes down what was said,
            with a timestamp on each line. That transcript is what the chaptrs
            pass reads later.
          </p>
          <p className="note">
            Give it the unusual words now. They cannot be corrected afterwards
            without transcribing again.
          </p>

          <Vocabulary value={notes} onChange={setNotes} />

          <div className="actions">
            <button onClick={onClose}>Cancel</button>
            <button className="accent" onClick={go} disabled={!files.length}>
              Transcribe
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
