import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../state/store";
import type { Prefs } from "../types";
import { ModelPicker, VadNotice } from "./ModelStore";

const PROVIDERS: { id: Prefs["cloud_provider"]; name: string; hint: string }[] = [
  { id: "anthropic", name: "Anthropic", hint: "claude-sonnet-4-20250514" },
  { id: "openai", name: "OpenAI", hint: "gpt-4.1-mini" },
  { id: "google", name: "Google", hint: "gemini-2.5-flash" },
  { id: "compatible", name: "OpenAI-compatible", hint: "needs a base URL" },
];

export function Settings({ onClose }: { onClose: () => void }) {
  const settings = useStore((s) => s.settings);
  const saveSettings = useStore((s) => s.saveSettings);
  const say = useStore((s) => s.say);
  const s = useStore();
  const [p, setP] = useState<Prefs | null>(null);

  useEffect(() => {
    invoke<Prefs>("get_prefs").then(setP);
  }, []);

  if (!p || !settings) return null;
  const set = (patch: Partial<Prefs>) => {
    setP({ ...p, ...patch });
    // Choosing a different model changes whether anything is missing.
    if ("local_model" in patch || "whisper_model" in patch || "engine" in patch) {
      queueMicrotask(() => s.recheck());
    }
  };

  const done = async () => {
    await invoke("save_prefs", { prefsIn: p });
    say("settings saved", "ok");
    onClose();
  };

  const num = (v: string, fallback: number) => (v === "" ? fallback : Number(v));

  return (
    <div className="sheet" onMouseDown={onClose}>
      <div className="sheet-body" onMouseDown={(e) => e.stopPropagation()}>
        <div className="pane-h">
          <span>Settings</span>
          <span className="spacer" />
          <button className="accent" onClick={done}>Done</button>
        </div>

        <div className="sheet-scroll">
          <VadNotice />

          <div className="layout">
            <div className="layout-h"><b>DaVinci Resolve</b></div>
            <div className="field">
              <b>Playback</b>
              <span className="note">
                Clicking a chaptr moves Resolve's playhead to that moment, if the
                recording is on the current timeline. Install the script, then start
                it once per session from Resolve's Workspace &gt; Scripts menu.
              </span>
              <div className="row">
                <button onClick={() => s.installResolve()}>
                  {s.resolveLink?.installed ? "Reinstall script" : "Install script"}
                </button>
                {s.resolveLink?.installed && <span className="ok">installed</span>}
              </div>
              {s.resolveLink?.scripts_dir && (
                <span className="note mono">{s.resolveLink.scripts_dir}</span>
              )}
              {s.resolveResult && (
                <span className={s.resolveResult.ok ? "note mono" : "note mono bad"}>
                  {s.resolveResult.ok
                    ? `wrote ${s.resolveResult.text}`
                    : s.resolveResult.text}
                </span>
              )}
            </div>
          </div>

          <div className="layout">
            <div className="layout-h"><b>Transcription</b></div>

            <ModelPicker
              kind="speech"
              label="Quality"
              chosen={p.whisper_model}
              onChoose={(whisper_model) => set({ whisper_model })}
            />

            <label className="field">
              <b>Language</b>
              <span className="note">Two-letter code. "auto" detects per file, more slowly.</span>
              <input value={p.language} onChange={(e) => set({ language: e.target.value })} />
            </label>

            <label className="field">
              <b>Voice detection threshold — {p.vad_threshold.toFixed(2)}</b>
              <span className="note">
                Silence is skipped rather than transcribed, which removes
                hallucinated repetition and roughly halves the time. Raise it if
                you get invented lines during quiet stretches; lower it if
                mumbled talk goes missing.
              </span>
              <input
                type="range" min={0.1} max={0.9} step={0.05}
                value={p.vad_threshold}
                onChange={(e) => set({ vad_threshold: Number(e.target.value) })}
              />
            </label>
          </div>

          <div className="layout">
            <div className="layout-h"><b>Chaptrs — this project</b></div>

            <label className="field">
              <b>Window — {settings.window_minutes} minutes</b>
              <span className="note">
                How much transcript the model reads at once. Long windows give
                better context but fewer, broader chaptrs. Drop to 2-3 minutes
                for short clips, or you get two chaptrs for the whole thing.
              </span>
              <input
                type="range" min={1} max={30} step={1}
                value={settings.window_minutes}
                onChange={(e) => saveSettings({ window_minutes: Number(e.target.value) })}
              />
            </label>

            <label className="field">
              <b>Overlap — {settings.overlap_minutes} minutes</b>
              <span className="note">
                Windows overlap so an event on a boundary is not missed.
                Duplicates are removed afterwards.
              </span>
              <input
                type="range" min={0} max={5} step={0.5}
                value={settings.overlap_minutes}
                onChange={(e) => saveSettings({ overlap_minutes: Number(e.target.value) })}
              />
            </label>

            <div className="field">
              <b>Chaptrs per window — {settings.min_per_window} to {settings.max_per_window}</b>
              <span className="note">
                The minimum matters more than it looks: allowed to return
                nothing, the model does so for about half of all windows. The
                maximum stops it padding with filler to fill the quota.
              </span>
              <div className="row">
                <input
                  type="number" min={0} max={10}
                  value={settings.min_per_window}
                  onChange={(e) => saveSettings({ min_per_window: num(e.target.value, 2) })}
                />
                <input
                  type="number" min={1} max={20}
                  value={settings.max_per_window}
                  onChange={(e) => saveSettings({ max_per_window: num(e.target.value, 6) })}
                />
              </div>
            </div>

            <label className="field">
              <b>Temperature — {settings.temperature.toFixed(2)}</b>
              <span className="note">
                Low keeps chaptrs literal and repeatable. Above about 0.6 it
                starts embellishing events that are not in the transcript.
              </span>
              <input
                type="range" min={0} max={1} step={0.05}
                value={settings.temperature}
                onChange={(e) => saveSettings({ temperature: Number(e.target.value) })}
              />
            </label>
          </div>

          <div className="layout">
            <div className="layout-h"><b>Language model</b></div>

            <div className="row tabs-inline">
              <button
                className={p.engine === "local" ? "on" : ""}
                onClick={() => set({ engine: "local" })}
              >
                On this machine
              </button>
              <button
                className={p.engine === "cloud" ? "on" : ""}
                onClick={() => set({ engine: "cloud" })}
              >
                Cloud API
              </button>
            </div>

            {p.engine === "local" && (
              <ModelPicker
                kind="language"
                label="Quality"
                chosen={p.local_model}
                onChoose={(local_model) => set({ local_model })}
              />
            )}

            {p.engine === "cloud" && (
              <>
                <p className="note">
                  Roughly 6 requests per hour of footage, about 1.5k tokens each.
                  A hundred hours is on the order of a million input tokens —
                  cheap on a small model, not free on a large one. Keys are
                  stored on this machine only and never go into a project file.
                </p>
                <label className="field">
                  <b>Provider</b>
                  <select
                    value={p.cloud_provider}
                    onChange={(e) => set({ cloud_provider: e.target.value as Prefs["cloud_provider"] })}
                  >
                    {PROVIDERS.map((x) => <option key={x.id} value={x.id}>{x.name}</option>)}
                  </select>
                </label>
                <label className="field">
                  <b>Model</b>
                  <span className="note">
                    e.g. {PROVIDERS.find((x) => x.id === p.cloud_provider)?.hint}
                  </span>
                  <input
                    className="wide"
                    value={p.cloud_model}
                    onChange={(e) => set({ cloud_model: e.target.value })}
                  />
                </label>
                <label className="field">
                  <b>API key</b>
                  <input
                    className="wide"
                    type="password"
                    placeholder="stored on this machine only"
                    value={
                      p.cloud_provider === "anthropic" ? p.anthropic_key
                      : p.cloud_provider === "google" ? p.google_key
                      : p.openai_key
                    }
                    onChange={(e) => {
                      const v = e.target.value;
                      if (p.cloud_provider === "anthropic") set({ anthropic_key: v });
                      else if (p.cloud_provider === "google") set({ google_key: v });
                      else set({ openai_key: v });
                    }}
                  />
                </label>
                {p.cloud_provider === "compatible" && (
                  <label className="field">
                    <b>Base URL</b>
                    <span className="note">Anything speaking the OpenAI chat format.</span>
                    <input
                      className="wide"
                      placeholder="https://openrouter.ai/api/v1"
                      value={p.cloud_base_url}
                      onChange={(e) => set({ cloud_base_url: e.target.value })}
                    />
                  </label>
                )}
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
