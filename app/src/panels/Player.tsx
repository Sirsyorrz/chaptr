import { useEffect, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useStore } from "../state/store";
import { hmsf } from "../types";

export function Player() {
  const ref = useRef<HTMLVideoElement>(null);
  const current = useStore((s) => s.current);
  const time = useStore((s) => s.time);
  const playing = useStore((s) => s.playing);
  const setTime = useStore((s) => s.setTime);
  const setPlaying = useStore((s) => s.setPlaying);
  const seeking = useRef(false);

  // The timeline is the source of truth for position; the element follows it
  // unless the element itself produced the change.
  useEffect(() => {
    const v = ref.current;
    if (!v || seeking.current) return;
    if (Math.abs(v.currentTime - time) > 0.25) v.currentTime = time;
  }, [time]);

  useEffect(() => {
    const v = ref.current;
    if (!v) return;
    if (playing) v.play().catch(() => setPlaying(false));
    else v.pause();
  }, [playing, setPlaying]);

  if (!current) return <div className="player empty">No clip selected.</div>;

  if (!current.proxy) {
    return (
      <div className="player empty">
        <p>No proxy for this clip.</p>
        <code>chaptr proxy --clip "{current.clip.clip.split("/").pop()}"</code>
      </div>
    );
  }

  const fps = current.clip.fps || 60;
  const step = (frames: number) => {
    setPlaying(false);
    setTime(Math.max(0, time + frames / fps));
  };

  return (
    <div className="player">
      <video
        ref={ref}
        src={convertFileSrc(current.proxy)}
        onTimeUpdate={(e) => {
          seeking.current = true;
          setTime((e.target as HTMLVideoElement).currentTime);
          seeking.current = false;
        }}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onClick={() => setPlaying(!playing)}
      />
      <div className="transport">
        <button onClick={() => setPlaying(!playing)} title="Play/pause (space)">
          {playing ? "❚❚" : "▶"}
        </button>
        <button onClick={() => step(-1)} title="Back one frame">‹</button>
        <button onClick={() => step(1)} title="Forward one frame">›</button>
        <button onClick={() => { setPlaying(false); setTime(Math.max(0, time - 10)); }}>−10s</button>
        <button onClick={() => { setPlaying(false); setTime(time + 10); }}>+10s</button>
        <span className="spacer" />
        <span className="mono dim">{hmsf(time, fps)}</span>
      </div>
    </div>
  );
}
