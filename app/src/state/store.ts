import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ClipDetail, ClipSummary, Marker, Tag } from "../types";
import { hms } from "../types";

const WS_KEY = "chaptr.workspace";

interface State {
  workspace: string;
  clips: ClipSummary[];
  current: ClipDetail | null;
  markers: Marker[];
  dirty: boolean;
  status: string;
  statusKind: "" | "ok" | "warn";
  time: number;
  playing: boolean;
  zoom: number;
  selected: number | null;
  error: string | null;
  loading: boolean;

  setWorkspace: (path: string) => Promise<void>;
  refresh: () => Promise<void>;
  open: (key: string) => Promise<void>;
  save: () => Promise<void>;
  revert: () => Promise<void>;

  setTime: (t: number) => void;
  setPlaying: (p: boolean) => void;
  setZoom: (z: number) => void;
  select: (i: number | null) => void;

  addMarker: (at: number, title?: string) => void;
  updateMarker: (i: number, patch: Partial<Marker>) => void;
  deleteMarker: (i: number) => void;
  cycleTag: (i: number) => void;
  setStatus: (msg: string, kind?: "" | "ok" | "warn") => void;
}

const TAGS: Tag[] = [
  "combat", "objective", "banter", "planning",
  "highlight", "death", "downtime", "meta",
];

export const useStore = create<State>((set, get) => ({
  workspace: localStorage.getItem(WS_KEY) || "",
  clips: [],
  current: null,
  markers: [],
  dirty: false,
  status: "",
  statusKind: "",
  time: 0,
  playing: false,
  zoom: 1,
  selected: null,
  error: null,
  loading: false,

  setStatus: (status, statusKind = "") => set({ status, statusKind }),

  setWorkspace: async (path) => {
    localStorage.setItem(WS_KEY, path);
    set({ workspace: path, current: null, markers: [], clips: [] });
    await get().refresh();
  },

  refresh: async () => {
    const { workspace } = get();
    if (!workspace) return;
    set({ loading: true, error: null });
    try {
      const clips = await invoke<ClipSummary[]>("list_clips", { ws: { root: workspace } });
      set({ clips, loading: false });
      if (clips.length && !get().current) await get().open(clips[0].key);
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  open: async (key) => {
    const { workspace, dirty } = get();
    if (dirty && !confirm("Discard unsaved marker changes?")) return;
    set({ loading: true });
    try {
      const detail = await invoke<ClipDetail>("load_clip", { ws: { root: workspace }, key });
      set({
        current: detail,
        markers: detail.markers,
        dirty: false,
        time: 0,
        selected: null,
        status: "",
        statusKind: "",
        loading: false,
      });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  save: async () => {
    const { workspace, current, markers } = get();
    if (!current) return;
    try {
      await invoke("save_markers", {
        ws: { root: workspace },
        key: current.key,
        markers,
      });
      set({ dirty: false, status: "saved", statusKind: "ok" });
      await get().refresh();
    } catch (e) {
      set({ status: `save failed: ${e}`, statusKind: "warn" });
    }
  },

  revert: async () => {
    const { workspace, current } = get();
    if (!current) return;
    if (!confirm("Discard your edits and restore the generated markers?")) return;
    await invoke("revert_markers", { ws: { root: workspace }, key: current.key });
    await get().open(current.key);
  },

  setTime: (time) => set({ time }),
  setPlaying: (playing) => set({ playing }),
  setZoom: (zoom) => set({ zoom: Math.min(64, Math.max(0.25, zoom)) }),
  select: (selected) => set({ selected }),

  addMarker: (at, title = "") => {
    const markers = [...get().markers, {
      seconds: at,
      t: hms(at),
      title,
      note: "",
      tag: "meta" as Tag,
      speakers: [],
      source: "manual" as const,
      confidence: 1,
    }].sort((a, b) => a.seconds - b.seconds);
    set({ markers, dirty: true, status: "unsaved", statusKind: "warn" });
  },

  updateMarker: (i, patch) => {
    const markers = get().markers.slice();
    markers[i] = { ...markers[i], ...patch };
    if (patch.seconds !== undefined) markers[i].t = hms(patch.seconds);
    set({ markers, dirty: true, status: "unsaved", statusKind: "warn" });
  },

  deleteMarker: (i) => {
    const markers = get().markers.slice();
    markers.splice(i, 1);
    set({ markers, dirty: true, selected: null, status: "unsaved", statusKind: "warn" });
  },

  cycleTag: (i) => {
    const cur = get().markers[i];
    const next = TAGS[(TAGS.indexOf(cur.tag) + 1) % TAGS.length];
    get().updateMarker(i, { tag: next });
  },
}));
