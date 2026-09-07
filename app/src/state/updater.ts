import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type Phase = "idle" | "checking" | "found" | "downloading" | "ready" | "error";

interface UpdaterState {
  phase: Phase;
  update: Update | null;
  percent: number;
  error: string;
  /// `loud` reports "already up to date" and failures; the check on startup
  /// stays silent so a flaky network never nags.
  look: (loud?: boolean) => Promise<void>;
  install: () => Promise<void>;
  restart: () => Promise<void>;
  dismiss: () => void;
}

export const useUpdater = create<UpdaterState>((set, get) => ({
  phase: "idle",
  update: null,
  percent: 0,
  error: "",

  look: async (loud = false) => {
    if (get().phase === "downloading") return;
    set({ phase: "checking", error: "" });
    try {
      const found = await check();
      if (found) set({ update: found, phase: "found" });
      else set({ phase: loud ? "error" : "idle", error: loud ? "already up to date" : "" });
    } catch (e) {
      set({ phase: loud ? "error" : "idle", error: loud ? String(e) : "" });
    }
  },

  install: async () => {
    const u = get().update;
    if (!u) return;
    set({ phase: "downloading", percent: 0 });
    let total = 0;
    let got = 0;
    try {
      await u.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") {
          got += e.data.chunkLength;
          if (total) set({ percent: Math.round((got / total) * 100) });
        }
      });
      set({ phase: "ready" });
    } catch (e) {
      set({ phase: "error", error: String(e) });
    }
  },

  restart: () => relaunch(),
  dismiss: () => set({ phase: "idle", error: "" }),
}));
