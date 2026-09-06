import { useState } from "react";
import { useStore } from "../state/store";

/**
 * Asked just before the language model runs, because this is the only place
 * the answers are used. Transcription never sees them.
 */
export function FindChaptrs({ onClose }: { onClose: () => void }) {
  const settings = useStore((s) => s.settings);
  const saveSettings = useStore((s) => s.saveSettings);
  const runJob = useStore((s) => s.runJob);
  const files = useStore((s) => s.files);

  const [subject, setSubject] = useState(settings?.subject ?? "");
  const [notes, setNotes] = useState(settings?.notes ?? "");

  const ready = files.filter((f) => f.transcribed).length;

  const go = async () => {
    await saveSettings({ subject, notes });
    onClose();
    runJob("chaptrs");
  };

  return (
    <div className="sheet" onMouseDown={onClose}>
      <div className="sheet-body narrow" onMouseDown={(e) => e.stopPropagation()}>
        <div className="pane-h">
          <span>Find chaptrs</span>
          <span className="spacer" />
          <span className="dim">{ready} transcribed</span>
        </div>

        <div className="sheet-scroll">
          <p className="note">
            chaptr reads the transcript in ten-minute windows and writes down
            what happened in each. These two answers are pasted into the
            instructions it gets. They do not affect transcription at all — only
            the wording of the chaptrs.
          </p>

          <label className="field">
            <b>What is this footage?</b>
            <span className="note">
              Sets the context sentence: "This recording is ___." It tells the
              model what kind of events to expect, so a game session yields
              kills and objectives while a meeting yields decisions. Leave it
              vague and you still get chaptrs, just blander ones.
            </span>
            <input
              className="wide"
              placeholder="a match of Deadlock, a hero shooter"
              value={subject}
              onChange={(e) => setSubject(e.target.value)}
            />
          </label>

          <label className="field">
            <b>Names and words it should know</b>
            <span className="note">
              Added as "Names and terms you may hear: ___." Speech-to-text
              mangles unusual names, and this gives the model the correct
              spellings to recognise. Worth listing the people you play with,
              plus any jargon that matters.
            </span>
            <input
              className="wide"
              placeholder="Josh, Haze, Abrams, ult, urn, mid"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
          </label>

          <p className="note">
            The rest of the instructions are fixed: report events rather than
            who spoke, use a real name only when the transcript says one, and
            keep each line short enough to scan.
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
