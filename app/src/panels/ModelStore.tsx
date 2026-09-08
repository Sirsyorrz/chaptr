import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { gb, type CatalogueEntry, type DownloadProgress } from "../types";
import { useStore } from "../state/store";

interface Props {
  kind: "speech" | "language";
  label: string;
  chosen: string;
  onChoose: (file: string) => void;
}

/** Filename to the model's actual published name, e.g. Qwen3-8B-Q4_K_M. */
const modelName = (file: string) =>
  file.replace(/\.(gguf|bin)$/i, "").replace(/^ggml-/, "");

/**
 * A quality ladder rather than a list of filenames. Picking a tier that is not
 * downloaded yet offers the download right there.
 */
export function ModelPicker({ kind, label, chosen, onChoose }: Props) {
  const [items, setItems] = useState<CatalogueEntry[]>([]);
  const [vram, setVram] = useState<number | null>(null);
  const [busy, setBusy] = useState<DownloadProgress | null>(null);
  const [failed, setFailed] = useState("");

  const refresh = () => invoke<CatalogueEntry[]>("list_catalogue").then(setItems);

  const start = (id: string) => {
    setFailed("");
    invoke("download_model", { id }).catch((e) => setFailed(String(e)));
  };

  useEffect(() => {
    refresh();
    invoke<number | null>("gpu_vram").then(setVram);
    const un = listen<DownloadProgress>("download", (e) => {
      const p = e.payload;
      setBusy(p.done ? null : p);
      if (p.error) setFailed(p.error);
      if (p.done) {
        refresh();
        useStore.getState().recheck();
      }
    });
    return () => { un.then((f) => f()); };
  }, []);

  const tiers = items.filter((i) => i.kind === kind).sort((a, b) => a.tier - b.tier);
  const current = tiers.find((t) => t.file === chosen) ?? tiers[1] ?? tiers[0];
  if (!current) return null;

  const active = busy?.id === current.id;
  const tooBig = vram !== null && current.vram > vram;
  const pct = busy?.total ? (busy.received / busy.total) * 100 : 0;

  return (
    <label className="field">
      <b>{label}</b>
      <select value={current.file} onChange={(e) => onChoose(e.target.value)}>
        {tiers.map((t) => (
          <option key={t.id} value={t.file}>
            {t.tier_name} — {modelName(t.file)} — {gb(t.bytes)}
            {t.installed ? "" : " (not downloaded)"}
          </option>
        ))}
      </select>

      <span className="note mono dim">{current.file}</span>
      <span className="note">{current.note}</span>
      <span className="note">
        {current.speed && <>Speed: {current.speed}. </>}
        {current.vram > 0 && <>Wants about {current.vram} GB of graphics memory.</>}
      </span>

      {tooBig && (
        <span className="note warn">
          This machine reports {vram} GB, so this may not fit and will fall back
          to the processor — very slow.
        </span>
      )}

      {active && (
        <div className="dlrow">
          <div className="bar"><div className="fill" style={{ width: `${pct}%` }} /></div>
          <span className="mono dim">{pct.toFixed(0)}%</span>
          <button onClick={() => invoke("cancel_download")}>Stop</button>
        </div>
      )}

      {!current.installed && !active && (
        <div className="dlrow">
          <button className="accent" disabled={!!busy}
            onClick={() => start(current.id)}>
            Download {gb(current.bytes)}
          </button>
          {busy && <span className="dim">another download is running</span>}
        </div>
      )}

      {failed && <span className="note warn">Download failed: {failed}</span>}

      {current.installed && !active && (
        <div className="dlrow">
          <span className="pill ok">downloaded</span>
          <button onClick={() => invoke("delete_model", { id: current.id }).then(refresh)}>
            Remove
          </button>
        </div>
      )}
    </label>
  );
}

/** The one model that is not a choice: nothing works without it. */
export function VadNotice() {
  const [entry, setEntry] = useState<CatalogueEntry | null>(null);
  const [busy, setBusy] = useState<DownloadProgress | null>(null);
  const [failed, setFailed] = useState("");

  const refresh = () =>
    invoke<CatalogueEntry[]>("list_catalogue").then((all) =>
      setEntry(all.find((e) => e.kind === "support") ?? null),
    );

  useEffect(() => {
    refresh();
    const un = listen<DownloadProgress>("download", (e) => {
      setBusy(e.payload.done ? null : e.payload);
      if (e.payload.error) setFailed(e.payload.error);
      if (e.payload.done) refresh();
    });
    return () => { un.then((f) => f()); };
  }, []);

  if (!entry || entry.installed) return null;
  return (
    <p className="note warn">
      Voice detection ({modelName(entry.file)}, {gb(entry.bytes)}) is missing. Transcripts will fill with
      repeated nonsense during silence without it.{" "}
      <button
        disabled={!!busy}
        onClick={() => {
          setFailed("");
          invoke("download_model", { id: entry.id }).catch((e) => setFailed(String(e)));
        }}
      >
        Download
      </button>
      {busy && !busy.done && <> {Math.round((busy.received / (busy.total || 1)) * 100)}%</>}
      {failed && <> Download failed: {failed}</>}
    </p>
  );
}
