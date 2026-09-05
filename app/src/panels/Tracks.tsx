import { useStore } from "../state/store";
import { ROLES, ROLE_HELP, type Role } from "../types";

/**
 * Which audio track is what. Detection proposes, the user decides — one folder
 * can hold recordings made with different OBS routing, so roles are per layout.
 */
export function Tracks({ onClose }: { onClose: () => void }) {
  const layouts = useStore((s) => s.layouts);
  const settings = useStore((s) => s.settings);
  const busy = useStore((s) => s.busy);
  const detect = useStore((s) => s.detect);
  const setRole = useStore((s) => s.setRole);
  const saveSettings = useStore((s) => s.saveSettings);

  const roleOf = (sig: string, i: number, fallback: Role): Role =>
    (settings?.roles[sig]?.[i] as Role) ?? fallback;

  return (
    <div className="sheet" onMouseDown={onClose}>
      <div className="sheet-body" onMouseDown={(e) => e.stopPropagation()}>
        <div className="pane-h">
          <span>Audio tracks</span>
          <span className="spacer" />
          <button onClick={detect} disabled={!!busy}>
            {busy ? busy : layouts.length ? "Detect again" : "Detect"}
          </button>
          <button onClick={onClose}>Close</button>
        </div>

        <div className="sheet-scroll">
          <p className="note">
            chaptr listens to a minute of each track and guesses. It can tell game
            audio apart reliably, but nothing in the audio says whether a voice
            track is your microphone or your friends — pick those yourself.
          </p>

          {!layouts.length && (
            <p className="pad dim">
              Run detect to see what each track contains.
              {settings && Object.keys(settings.roles).length > 0 &&
                " Saved roles are already in use."}
            </p>
          )}

          {layouts.map((l) => (
            <div className="layout" key={l.signature}>
              <div className="layout-h">
                <b>{l.signature}</b>
                <span className="dim">
                  {l.recordings} recording{l.recordings === 1 ? "" : "s"} · e.g. {l.example}
                </span>
              </div>
              {l.tracks.map((t) => {
                const role = roleOf(l.signature, t.index, t.role);
                return (
                  <div className={"trow" + (role === "unknown" ? " unsure" : "")} key={t.index}>
                    <span className="tname">
                      {t.index}. {t.name}
                    </span>
                    <span className="wpm mono" title="Words per minute heard">
                      {t.words_per_minute.toFixed(0)} wpm
                    </span>
                    <select
                      value={role}
                      onChange={(e) => setRole(l.signature, t.index, e.target.value as Role)}
                      title={ROLE_HELP[role]}
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>{r}</option>
                      ))}
                    </select>
                    <span className="tsample dim" title={t.sample}>
                      {t.sample.slice(0, 110) || "— nothing heard —"}
                    </span>
                  </div>
                );
              })}
            </div>
          ))}

          <div className="layout">
            <div className="layout-h"><b>What this footage is</b></div>
            <p className="note">
              Used to word the prompt. Nothing else in chaptr is game-specific.
            </p>
            <input
              className="wide"
              placeholder="a match of Deadlock, a hero shooter"
              defaultValue={settings?.subject ?? ""}
              onBlur={(e) => saveSettings({ subject: e.target.value })}
            />
            <input
              className="wide"
              placeholder="names and jargon: Haze, Abrams, ult, urn"
              defaultValue={settings?.notes ?? ""}
              onBlur={(e) => saveSettings({ notes: e.target.value })}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
