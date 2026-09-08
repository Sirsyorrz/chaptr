import { useEffect } from "react";
import { useStore } from "../state/store";
import { ROLES, ROLE_HELP, ROLE_LABEL, type Role } from "../types";

/**
 * Which audio track is what. Roles can simply be set: a recording setup rarely
 * changes, so detection is offered rather than required.
 */
export function Tracks({ onClose }: { onClose: () => void }) {
  const layouts = useStore((s) => s.layouts);
  const settings = useStore((s) => s.settings);
  const busy = useStore((s) => s.busy);
  const detect = useStore((s) => s.detect);
  const setRole = useStore((s) => s.setRole);
  const loadLayouts = useStore((s) => s.loadLayouts);

  useEffect(() => {
    if (!layouts.length) loadLayouts();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const roleOf = (sig: string, i: number, fallback: Role): Role =>
    (settings?.roles[sig]?.[i] as Role) ?? fallback;

  return (
    <div className="sheet" onMouseDown={onClose}>
      <div className="sheet-body" onMouseDown={(e) => e.stopPropagation()}>
        <div className="pane-h">
          <span>Audio tracks</span>
          <span className="spacer" />
          <button onClick={detect} disabled={!!busy}>
            {busy ? busy : "Listen and guess"}
          </button>
          <button onClick={onClose}>Done</button>
        </div>

        <div className="sheet-scroll">
          <p className="note">
            Tell chaptr what each track holds, so it knows which ones carry
            speech and who is speaking. Recording setups rarely change, so this
            is usually a one-off.
          </p>
          <p className="note">
            "Listen and guess" samples three minutes from one recording per
            layout rather than scanning the whole project, so it takes seconds
            even on a hundred hours.
          </p>
          <p className="note">
            The pair that matters is <b>me</b> and <b>friends</b>. Set both and
            chaptr transcribes them separately, so a chaptr can say who did
            what. With only <b>everything mixed</b> it still works, but nothing
            identifies the speaker and chaptrs come out as "a player".
          </p>

          {layouts.map((l) => (
            <div className="layout" key={l.signature}>
              <div className="layout-h">
                <b>{l.track_count} tracks</b>
                <span className="dim">
                  {l.recordings} recording{l.recordings === 1 ? "" : "s"} · e.g. {l.example}
                </span>
              </div>
              {l.tracks.map((t) => {
                const role = roleOf(l.signature, t.index, t.role);
                const heard = t.words_per_minute >= 0;
                return (
                  <div className={"trow" + (role === "unknown" ? " unsure" : "")} key={t.index}>
                    <span className="tname">
                      {t.index + 1}. {t.name}
                    </span>
                    <span className="wpm mono" title="Words per minute heard">
                      {heard ? `${t.words_per_minute.toFixed(0)} wpm` : ""}
                    </span>
                    <select
                      value={role}
                      onChange={(e) => setRole(l.signature, t.index, e.target.value as Role)}
                      title={ROLE_HELP[role]}
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>{ROLE_LABEL[r]}</option>
                      ))}
                    </select>
                    <span className="tsample dim" title={t.sample}>
                      {heard
                        ? t.sample.slice(0, 110) || "nothing heard"
                        : `${t.channels} channel${t.channels === 1 ? "" : "s"}`}
                    </span>
                  </div>
                );
              })}
            </div>
          ))}

          {!layouts.length && <p className="pad dim">Import some recordings first.</p>}
        </div>
      </div>
    </div>
  );
}
