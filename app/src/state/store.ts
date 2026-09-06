import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Chaptr, FileStatus, JobProgress, Layout, Library, Project, Role, Settings, Transcript } from "../types";

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
  runId: number;
  files: FileStatus[];
  viewing: string | null;
  tab: "chaptrs" | "transcribes";
  setTab: (t: "chaptrs" | "transcribes") => void;
  loadLayouts: () => Promise<void>;
  refreshFiles: () => Promise<void>;
  openFile: (recordingId: string) => Promise<void>;
  boot: () => Promise<void>;
  runJob: (stage: "transcribe" | "chaptrs") => Promise<void>;
  cancelJob: () => Promise<void>;
  onJob: (p: JobProgress) => void;
  openSources: (sources: string[]) => Promise<void>;
  openProject: (id: string) => Promise<void>;
  closeProject: (discard: boolean) => Promise<void>;
  confirmDiscard: () => boolean;
  forget: (id: string, deleteData: boolean) => Promise<void>;
  saveProject: (path?: string) => Promise<void>;
  openFile_: (path: string) => Promise<void>;
  rescan: () => Promise<void>;
  detect: () => Promise<void>;
  setRole: (signature: string, index: number, role: Role) => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;

  resolveLink: { installed: boolean; scripts_dir: string } | null;
  checkResolve: () => Promise<void>;
  installResolve: () => Promise<void>;
  gotoResolve: (recordingId: string, offset: number) => void;
  select: (i: number | null) => Promise<void>;
  setQuery: (q: string) => void;
  toggleStarredOnly: () => void;
  star: (i: number) => void;
  remove: (i: number) => void;
  save: () => Promise<void>;
  say: (msg: string, kind?: "" | "ok" | "warn") => void;
}

export const useStore = create<State>((set, get) => ({
  project: null,
  projects: [],
  resolveLink: null,
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
  runId: 0,
  files: [],
  viewing: null,
  tab: "chaptrs",

  say: (status, statusKind = "") => set({ status, statusKind }),

  setTab: (tab) => set({ tab }),

  /// Layouts without listening to anything, so roles can be set by hand.
  loadLayouts: async () => {
    const id = get().project?.id;
    if (!id) return;
    set({ layouts: await invoke<Layout[]>("list_layouts", { id }) });
  },

  refreshFiles: async () => {
    const id = get().project?.id;
    if (!id) return;
    set({ files: await invoke<FileStatus[]>("file_status", { id }) });
  },

  openFile: async (recordingId) => {
    const id = get().project?.id;
    if (!id) return;
    const t = await invoke<Transcript | null>("load_transcript", { id, recordingId });
    set({ transcript: t, viewing: recordingId, selected: null });
    if (!t) get().say("that recording has no transcript yet", "warn");
  },

  runJob: async (stage) => {
    const id = get().project?.id;
    if (!id) return;
    set({ job: null, busy: stage });
    try {
      await invoke("start_job", { id, stage });
      const poll = setInterval(async () => {
        const p = await invoke<JobProgress | null>("job_status");
        if (p) get().onJob(p);
        if (!get().busy) clearInterval(poll);
      }, 500);
      // Safety net: a job that never reports must not wedge the UI forever.
      setTimeout(() => clearInterval(poll), 1000 * 60 * 60 * 12);
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
    // Ignore anything left over from an earlier run.
    if (p.run < get().runId) return;
    set({ job: p, runId: p.run });
    if (p.error) get().say(p.error, "warn");
    if (!p.done) return;
    set({ busy: "", job: null });
    if (p.cancelled) get().say("stopped", "warn");
    else if (!p.error) get().say(p.message || "finished", "ok");
    const id = get().project?.id;
    if (!id) return;
    get().refreshFiles();
    invoke<Chaptr[]>("load_chaptrs", { id }).then((chaptrs) => set({ chaptrs, dirty: false }));
    invoke<Project[]>("list_projects").then((projects) =>
      set({ projects, project: projects.find((x) => x.id === id) ?? get().project }),
    );
  },

  boot: async () => {
    set({ missing: await invoke<string[]>("check_sidecars") });
    get().checkResolve();
    const projects = await invoke<Project[]>("list_projects");
    set({ projects });
    const last = localStorage.getItem(PROJECT_KEY);
    const pick = projects.find((p) => p.id === last) ?? projects[0];
    if (pick) await get().openProject(pick.id);
  },

  openSources: async (sources) => {
    if (!get().confirmDiscard()) return;
    set({ busy: "opening" });
    try {
      const p = await invoke<Project>("open_project", { sources });
      set({ projects: await invoke<Project[]>("list_projects") });
      await get().openProject(p.id);
      // Importing footage always means scanning it; there is nothing to look
      // at until we have.
      if (!get().library) await get().rescan();
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
      set({ project, projects, library, settings, chaptrs, layouts: [], dirty: false, busy: "",
            viewing: null, transcript: null });
      get().refreshFiles();
      if (!library) get().say("not scanned yet — press Scan", "warn");
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  /// One project at a time: warn before replacing unsaved work.
  confirmDiscard: () => {
    const p = get().project;
    if (!p?.unsaved) return true;
    return confirm(
      `"${p.name}" has work that is not in a saved file yet.\n\nClose it anyway?`,
    );
  },

  closeProject: async (discard) => {
    const p = get().project;
    if (p) await invoke("close_project", { id: p.id, discard });
    localStorage.removeItem(PROJECT_KEY);
    set({
      project: null, library: null, settings: null, layouts: [], chaptrs: [],
      files: [], transcript: null, selected: null, viewing: null,
      dirty: false, query: "", status: "", statusKind: "",
    });
  },

  saveProject: async (path) => {
    const p = get().project;
    if (!p) return;
    if (!path && !p.file) return get().say("pick a location with Save As", "warn");
    set({ busy: "saving" });
    try {
      const where = await invoke<string>("save_project", { id: p.id, path: path ?? null });
      const projects = await invoke<Project[]>("list_projects");
      set({
        projects,
        project: projects.find((x) => x.id === p.id) ?? null,
        busy: "",
      });
      get().say(`saved to ${where.split("/").pop()}`, "ok");
    } catch (e) {
      set({ busy: "" });
      get().say(String(e), "warn");
    }
  },

  openFile_: async (path) => {
    if (!get().confirmDiscard()) return;
    set({ busy: "opening" });
    try {
      const p = await invoke<Project>("open_project_file", { path });
      set({ busy: "" });
      await get().openProject(p.id);
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
      get().refreshFiles();
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
    const c = chaptrs[i];
    const id = c?.recording_id;
    if (id) get().gotoResolve(id, c.offset);
    if (!id || transcript?.recording_id === id) return;
    const t = await invoke<Transcript | null>("load_transcript", { id: project!.id, recordingId: id });
    set({ transcript: t });
  },

  checkResolve: async () => {
    try {
      set({ resolveLink: await invoke("resolve_link") });
    } catch {
      set({ resolveLink: null });
    }
  },

  installResolve: async () => {
    try {
      const at = await invoke<string>("install_resolve_link");
      get().say(`installed to ${at} — start it from Workspace > Scripts`, "ok");
      get().checkResolve();
    } catch (e) {
      get().say(String(e), "warn");
    }
  },

  /// Fire and forget. Resolve may not be running, and that is not an error.
  gotoResolve: (recordingId, offset) => {
    const id = get().project?.id;
    if (!id) return;
    invoke("resolve_goto", { id, recording: recordingId, offset }).catch(() => {});
  },

  setQuery: (query) => set({ query }),
  toggleStarredOnly: () => set({ starredOnly: !get().starredOnly }),

  star: (i) => {
    const chaptrs = get().chaptrs.slice();
    chaptrs[i] = { ...chaptrs[i], starred: !chaptrs[i].starred };
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
