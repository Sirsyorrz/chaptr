import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { gb, type CatalogueEntry, type DownloadProgress } from "../types";

/**
 * A short curated list rather than a model browser. Someone setting this up for
 * the first time should be able to press one button and get on with it.
 */
export function ModelStore() {
  const [items, setItems] = useState<CatalogueEntry[]>([]);
  const [vram, setVram] = useState<number | null>(null);
  const [busy, setBusy] = useState<DownloadProgress | null>(null);

  const refresh = () => invoke<CatalogueEntry[]>("list_catalogue").then(setItems);

  useEffect(() => {
    refresh();
    invoke<number | null>("gpu_vram").then(setVram);
    const un = listen<DownloadProgress>("download", (e) => {
      const p = e.payload;
      setBusy(p.done ? null : p);
      if (p.done) refresh();
    });
    return () => { un.then((f) => f()); };
  }, []);

  const missingRequired = items.filter((i) => i.required && !i.installed);

  return (
    <div className="layout">
      <div className="layout-h">
        <b>Models</b>
        <span className="dim">
          {vram ? `${vram} GB graphics memory` : "graphics memory unknown"}
        </span>
      </div>

      {missingRequired.length > 0 && (
        <p className="note warn">
          {missingRequired.length} required model
          {missingRequired.length === 1 ? "" : "s"} still to download —
          chaptr cannot transcribe or find chaptrs without them.
        </p>
      )}

      {items.map((m) => {
        const active = busy?.id === m.id;
        const tooBig = vram !== null && m.vram > vram;
        return (
          <div className="mrow2" key={m.id}>
            <div className="minfo">
              <b>
                {m.name}
                {m.required && <span className="req">needed</span>}
                {tooBig && <span className="req big">needs {m.vram} GB</span>}
              </b>
              <span className="note">{m.note}</span>
              {active && (
                <div className="bar">
                  <div
                    className="fill"
                    style={{ width: `${busy.total ? (busy.received / busy.total) * 100 : 0}%` }}
                  />
                </div>
              )}
            </div>
            <span className="mono dim">{gb(m.bytes)}</span>
            {active ? (
              <button onClick={() => invoke("cancel_download")}>
                {busy.total ? `${((busy.received / busy.total) * 100).toFixed(0)}%` : "…"} stop
              </button>
            ) : m.installed ? (
              <button
                onClick={() => invoke("delete_model", { id: m.id }).then(refresh)}
                title="Remove from disk"
              >
                Remove
              </button>
            ) : (
              <button
                className={m.required ? "accent" : ""}
                disabled={!!busy}
                onClick={() => invoke("download_model", { id: m.id })}
              >
                Download
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
