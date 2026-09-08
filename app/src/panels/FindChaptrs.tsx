import { useState } from "react";
import { useStore } from "../state/store";

/**
 * Asked just before the language model runs. The word list is not editable
 * here: it is set at transcribe time, and changing it afterwards could not fix
 * a transcript that is already written.
 */
export function FindChaptrs({ onClose }: { onClose: () => void }) {
  const settings = useStore((s) => s.settings);
  const saveSettings = useStore((s) => s.saveSettings);
  const runJob = useStore((s) => s.runJob);
  const files = useStore((s) => s.files);

  const [subject, setSubject] = useState(settings?.subject ?? "");
  const notes = settings?.notes?.trim() ?? "";

  const ready = files.filter((f) => f.transcribed).length;

  const go = async () => {
    await saveSettings({ subject });
    onClose();
    runJob("chaptrs");
  };

  return (
    <div className="sheet" onMouseDown={onClose}>
      <div className="sheet-body narrow" onMouseDown={(e) => e.stopPropagation()}>
        <div className="pane-h">
          <span>Chaptrs</span>
          <span className="spacer" />
          <span className="dim">{ready} transcribed</span>
        </div>

        <div className="sheet-scroll">
          <p className="note">
            chaptr reads the transcript in ten-minute windows and notes what
            happened in each, so you get a timestamped list of moments to jump
            back to. Your answer below goes into the instructions it gets.
          </p>

          <label className="field">
            <b>What is this footage?</b>
            <span className="note">
              Completes the sentence "This recording is ___." It tells the model
              what kind of events to expect, so a game session yields fights and
              objectives while a meeting yields decisions. Leave it vague and
              you still get chaptrs, just blander ones.
            </span>
            <input
              className="wide"
              placeholder="a game session, a podcast, a work call"
              value={subject}
              onChange={(e) => setSubject(e.target.value)}
            />
          </label>

          <div className="field">
            <b>Names and words it knows</b>
            <span className="note">
              Taken from what you entered before transcribing. It seeds the list
              of names the model may use, alongside the ones it picked out of
              the transcript itself.
            </span>
            <span className="dim">{notes || "none given"}</span>
          </div>

          <p className="note">
            The rest of the instructions are fixed: report what happened rather
            than who spoke, use a real name only when the transcript says one,
            and keep every line short enough to scan.
          </p>

          <div className="actions">
            <button onClick={onClose}>Cancel</button>
            <button className="accent" onClick={go} disabled={!ready}>
              {ready ? "Find chaptrs" : "Nothing transcribed yet"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
