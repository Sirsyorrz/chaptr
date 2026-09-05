import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Chaptr, JobProgress, Layout, Library, Project, Role, Settings, Transcript } from "../types";

const PROJECT_KEY = "chaptr.project";

interface State {
  project: Project | null;
  projects: Project[];
  library: Library | null;
  settings: Settings | null;
  layouts: Layout[];
  chaptrs: Chaptr[];
  transcript: Transcript | null;
  selected: number | null;
  query: string;
  starredOnly: boolean;
  dirty: boolean;
  busy: string;
  status: string;
  statusKind: "" | "ok" | "warn";
  missing: string[];

  job: JobProgress | null;
  boot: () => Promise<void>;
  runJob: (stage: "transcribe" | "chaptrs") => Promise<void>;
  cancelJob: () => Promise<void>;
  onJob: (p: JobProgress) => void;
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
  chaptrs: [],
  transcript: null,
  selected: null,
  query: "",
  starredOnly: false,
  dirty: false,
  busy: "",
  status: "",
  statusKind: "",
  missing: [],
  job: null,

  say: (status, statusKind = "") => set({ status, statusKind }),

  runJob: async (stage) => {
    const id = get().project?.id;
    if (!id) return;
    set({ job: null, busy: stage });
    try {
      await invoke("start_job", { id, stage });
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  cancelJob: async () => {
    await invoke("cancel_job");
    get().say("stopping after this file…", "warn");
  },

  /// Streamed from the backend while a pass runs.
  onJob: (p) => {
    set({ job: p });
    if (p.error) get().say(p.error, "warn");
    if (!p.done) return;
    set({ busy: "", job: null });
    if (p.cancelled) get().say("stopped", "warn");
    else if (!p.error) get().say(p.message || "finished", "ok");
    const id = get().project?.id;
    if (id && p.stage === "chaptrs") {
      invoke<Chaptr[]>("load_chaptrs", { id }).then((chaptrs) => set({ chaptrs, dirty: false }));
    }
  },

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
    set({ busy: "loading", selected: null, transcript: null, chaptrs: [] });
    try {
      const projects = await invoke<Project[]>("list_projects");
      const project = projects.find((p) => p.id === id) ?? null;
      const library = await invoke<Library | null>("load_library", { id });
      const settings = await invoke<Settings>("get_settings", { id });
      const chaptrs = await invoke<Chaptr[]>("load_chaptrs", { id });
      set({ project, projects, library, settings, chaptrs, layouts: [], dirty: false, busy: "" });
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
      set({ project: null, library: null, chaptrs: [], transcript: null });
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
    const { chaptrs, project, transcript } = get();
    if (i === null) return;
    const id = chaptrs[i]?.recording_id;
    if (!id || transcript?.recording_id === id) return;
    const t = await invoke<Transcript | null>("load_transcript", { id: project!.id, recordingId: id });
    set({ transcript: t });
  },

  setQuery: (query) => set({ query }),
  toggleStarredOnly: () => set({ starredOnly: !get().starredOnly }),

  star: (i) => {
    const chaptrs = get().chaptrs.slice();
    chaptrs[i] = { ...chaptrs[i], starred: !chaptrs[i].starred };
    set({ chaptrs, dirty: true });
  },

  edit: (i, text) => {
    const chaptrs = get().chaptrs.slice();
    chaptrs[i] = { ...chaptrs[i], text };
    set({ chaptrs, dirty: true });
  },

  remove: (i) => {
    const chaptrs = get().chaptrs.slice();
    chaptrs.splice(i, 1);
    set({ chaptrs, dirty: true, selected: null });
  },

  save: async () => {
    const { project, chaptrs } = get();
    if (!project) return;
    try {
      await invoke("save_chaptrs", { id: project.id, chaptrs });
      set({ dirty: false });
      get().say("saved", "ok");
    } catch (e) {
      get().say(String(e), "warn");
    }
  },
}));
