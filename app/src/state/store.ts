import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Beat, Layout, Library, Project, Role, Settings, Transcript } from "../types";

const PROJECT_KEY = "chaptr.project";

interface State {
  project: Project | null;
  projects: Project[];
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
  openSources: (sources: string[]) => Promise<void>;
  openProject: (id: string) => Promise<void>;
  forget: (id: string, deleteData: boolean) => Promise<void>;
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
  project: null,
  projects: [],
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
    const projects = await invoke<Project[]>("list_projects");
    set({ projects });
    const last = localStorage.getItem(PROJECT_KEY);
    const pick = projects.find((p) => p.id === last) ?? projects[0];
    if (pick) await get().openProject(pick.id);
  },

  openSources: async (sources) => {
    set({ busy: "opening" });
    try {
      const p = await invoke<Project>("open_project", { sources });
      set({ projects: await invoke<Project[]>("list_projects") });
      await get().openProject(p.id);
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  openProject: async (id) => {
    localStorage.setItem(PROJECT_KEY, id);
    set({ busy: "loading", selected: null, transcript: null, beats: [] });
    try {
      const projects = await invoke<Project[]>("list_projects");
      const project = projects.find((p) => p.id === id) ?? null;
      const library = await invoke<Library | null>("load_library", { id });
      const settings = await invoke<Settings>("get_settings", { id });
      const beats = await invoke<Beat[]>("load_beats", { id });
      set({ project, projects, library, settings, beats, layouts: [], dirty: false, busy: "" });
      if (!library) get().say("not scanned yet — press Scan", "warn");
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  forget: async (id, deleteData) => {
    await invoke("forget_project", { id, deleteData });
    const projects = await invoke<Project[]>("list_projects");
    set({ projects });
    if (get().project?.id === id) {
      set({ project: null, library: null, beats: [], transcript: null });
      if (projects[0]) await get().openProject(projects[0].id);
    }
  },

  rescan: async () => {
    const id = get().project?.id;
    if (!id) return;
    set({ busy: "scanning" });
    try {
      const res = await invoke<{ library: Library; problems: string[] }>("scan_project", { id });
      set({ library: res.library, busy: "", projects: await invoke<Project[]>("list_projects") });
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
    const id = get().project?.id;
    if (!id) return;
    set({ busy: "listening to tracks" });
    try {
      const layouts = await invoke<Layout[]>("detect_tracks", { id });
      const settings = await invoke<Settings>("get_settings", { id });
      set({ layouts, settings, busy: "" });
      const unsure = layouts.flatMap((l) => l.tracks).filter((t) => !t.confident).length;
      get().say(unsure ? `${unsure} tracks need confirming` : "tracks identified", unsure ? "warn" : "ok");
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  setRole: async (signature, index, role) => {
    const { project, settings } = get();
    if (!settings || !project) return;
    const roles = [...(settings.roles[signature] ?? [])];
    roles[index] = role;
    const next = { ...settings, roles: { ...settings.roles, [signature]: roles } };
    set({ settings: next });
    await invoke("set_roles", { id: project.id, signature, roles });
    get().say("track roles saved", "ok");
  },

  saveSettings: async (patch) => {
    const { project, settings } = get();
    if (!settings || !project) return;
    const next = { ...settings, ...patch };
    set({ settings: next });
    await invoke("save_settings", { id: project.id, settings: next });
  },

  select: async (i) => {
    set({ selected: i });
    const { beats, project, transcript } = get();
    if (i === null) return;
    const id = beats[i]?.recording_id;
    if (!id || transcript?.recording_id === id) return;
    const t = await invoke<Transcript | null>("load_transcript", { id: project!.id, recordingId: id });
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
    const { project, beats } = get();
    if (!project) return;
    try {
      await invoke("save_beats", { id: project.id, beats });
      set({ dirty: false });
      get().say("saved", "ok");
    } catch (e) {
      get().say(String(e), "warn");
    }
  },
}));
