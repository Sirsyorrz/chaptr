import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Beat, Layout, Library, Role, Settings, Transcript } from "../types";

const FOLDER_KEY = "chaptr.folder";

interface State {
  folder: string;
  library: Library | null;
  settings: Settings | null;
  layouts: Layout[];
  beats: Beat[];
  transcript: Transcript | null;
  selected: number | null;
  query: string;
  starredOnly: boolean;
  dirty: boolean;
  busy: string;
  status: string;
  statusKind: "" | "ok" | "warn";
  missing: string[];

  boot: () => Promise<void>;
  openFolder: (folder: string) => Promise<void>;
  rescan: () => Promise<void>;
  detect: () => Promise<void>;
  setRole: (signature: string, index: number, role: Role) => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;

  select: (i: number | null) => Promise<void>;
  setQuery: (q: string) => void;
  toggleStarredOnly: () => void;
  star: (i: number) => void;
  edit: (i: number, text: string) => void;
  remove: (i: number) => void;
  save: () => Promise<void>;
  say: (msg: string, kind?: "" | "ok" | "warn") => void;
}

export const useStore = create<State>((set, get) => ({
  folder: localStorage.getItem(FOLDER_KEY) || "",
  library: null,
  settings: null,
  layouts: [],
  beats: [],
  transcript: null,
  selected: null,
  query: "",
  starredOnly: false,
  dirty: false,
  busy: "",
  status: "",
  statusKind: "",
  missing: [],

  say: (status, statusKind = "") => set({ status, statusKind }),

  boot: async () => {
    set({ missing: await invoke<string[]>("check_sidecars") });
    const { folder } = get();
    if (folder) await get().openFolder(folder);
  },

  openFolder: async (folder) => {
    localStorage.setItem(FOLDER_KEY, folder);
    set({ folder, busy: "loading", selected: null, transcript: null });
    try {
      const library = await invoke<Library | null>("load_library", { folder });
      const settings = await invoke<Settings>("get_settings", { folder });
      const beats = await invoke<Beat[]>("load_beats", { folder });
      set({ library, settings, beats, dirty: false, busy: "" });
      if (!library) get().say("no library yet — scan the folder", "warn");
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  rescan: async () => {
    const { folder } = get();
    set({ busy: "scanning" });
    try {
      const res = await invoke<{ library: Library; problems: string[] }>("scan_folder", { folder });
      set({ library: res.library, busy: "" });
      get().say(
        `${res.library.recordings.length} recordings` +
          (res.problems.length ? `, ${res.problems.length} skipped` : ""),
        "ok",
      );
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  detect: async () => {
    const { folder } = get();
    set({ busy: "listening to tracks" });
    try {
      const layouts = await invoke<Layout[]>("detect_tracks", { folder });
      const settings = await invoke<Settings>("get_settings", { folder });
      set({ layouts, settings, busy: "" });
      const unsure = layouts.flatMap((l) => l.tracks).filter((t) => !t.confident).length;
      get().say(unsure ? `${unsure} tracks need confirming` : "tracks identified", unsure ? "warn" : "ok");
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  setRole: async (signature, index, role) => {
    const { folder, settings } = get();
    if (!settings) return;
    const roles = [...(settings.roles[signature] ?? [])];
    roles[index] = role;
    const next = { ...settings, roles: { ...settings.roles, [signature]: roles } };
    set({ settings: next });
    await invoke("set_roles", { folder, signature, roles });
    get().say("track roles saved", "ok");
  },

  saveSettings: async (patch) => {
    const { folder, settings } = get();
    if (!settings) return;
    const next = { ...settings, ...patch };
    set({ settings: next });
    await invoke("save_settings", { folder, settings: next });
  },

  select: async (i) => {
    set({ selected: i });
    const { beats, folder, transcript } = get();
    if (i === null) return;
    const id = beats[i]?.recording_id;
    if (!id || transcript?.recording_id === id) return;
    const t = await invoke<Transcript | null>("load_transcript", { folder, recordingId: id });
    set({ transcript: t });
  },

  setQuery: (query) => set({ query }),
  toggleStarredOnly: () => set({ starredOnly: !get().starredOnly }),

  star: (i) => {
    const beats = get().beats.slice();
    beats[i] = { ...beats[i], starred: !beats[i].starred };
    set({ beats, dirty: true });
  },

  edit: (i, text) => {
    const beats = get().beats.slice();
    beats[i] = { ...beats[i], text };
    set({ beats, dirty: true });
  },

  remove: (i) => {
    const beats = get().beats.slice();
    beats.splice(i, 1);
    set({ beats, dirty: true, selected: null });
  },

  save: async () => {
    const { folder, beats } = get();
    try {
      await invoke("save_beats", { folder, beats });
      set({ dirty: false });
      get().say("saved", "ok");
    } catch (e) {
      get().say(String(e), "warn");
    }
  },
}));
