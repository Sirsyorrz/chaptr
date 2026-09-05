import { useEffect, useRef, useCallback } from "react";
import { useStore } from "../state/store";
import { hms, type Marker } from "../types";

const LANE_H = 54;
const RULER_H = 20;
const MARKER_H = 16;
const LABEL_W = 62;

const TAG_COLOUR: Record<string, string> = {
  combat: "#ff9b8a",
  objective: "#4ea1ff",
  banter: "#4ade80",
  planning: "#8a94a2",
  highlight: "#ffcc4e",
  death: "#f87171",
  downtime: "#5d6673",
  meta: "#8a94a2",
};

/** Pick a tick spacing that yields a readable label every ~90px. */
function tickStep(pxPerSec: number): number {
  const target = 90 / pxPerSec;
  const steps = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];
  return steps.find((s) => s >= target) ?? 3600;
}

export function Timeline() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const current = useStore((s) => s.current);
  const markers = useStore((s) => s.markers);
  const time = useStore((s) => s.time);
  const zoom = useStore((s) => s.zoom);
  const selected = useStore((s) => s.selected);
  const setTime = useStore((s) => s.setTime);
  const setZoom = useStore((s) => s.setZoom);
  const select = useStore((s) => s.select);
  const updateMarker = useStore((s) => s.updateMarker);
  const dragging = useRef<{ kind: "playhead" | "marker"; index?: number } | null>(null);

  const duration = current?.clip.duration ?? 0;
  const peaks = current?.peaks;
  const roles = peaks ? Object.keys(peaks.tracks) : [];
  const height = RULER_H + MARKER_H + roles.length * LANE_H;

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap || !duration) return;

    const width = wrap.clientWidth;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr;
      canvas.height = height * dpr;
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const view = duration / zoom;
    const start = Math.max(0, Math.min(duration - view, time - view / 2));
    const plot = width - LABEL_W;
    const pps = plot / view;
    const x = (t: number) => LABEL_W + (t - start) * pps;

    ctx.fillStyle = "#0b0e13";
    ctx.fillRect(0, 0, width, height);

    // ruler
    ctx.fillStyle = "#141920";
    ctx.fillRect(0, 0, width, RULER_H);
    ctx.strokeStyle = "#232b36";
    ctx.beginPath();
    ctx.moveTo(0, RULER_H + 0.5);
    ctx.lineTo(width, RULER_H + 0.5);
    ctx.stroke();

    const step = tickStep(pps);
    ctx.font = "10px ui-monospace, monospace";
    ctx.textBaseline = "middle";
    for (let t = Math.floor(start / step) * step; t < start + view; t += step) {
      const px = x(t);
      if (px < LABEL_W) continue;
      ctx.strokeStyle = "#2a3441";
      ctx.beginPath();
      ctx.moveTo(Math.floor(px) + 0.5, 0);
      ctx.lineTo(Math.floor(px) + 0.5, height);
      ctx.stroke();
      ctx.fillStyle = "#5d6673";
      ctx.fillText(hms(t), px + 4, RULER_H / 2);
    }

    // waveform lanes
    roles.forEach((role, i) => {
      const top = RULER_H + MARKER_H + i * LANE_H;
      const mid = top + LANE_H / 2;
      const data = peaks!.tracks[role];
      const rate = peaks!.rate;

      ctx.fillStyle = i % 2 ? "#10151b" : "#0e1116";
      ctx.fillRect(LABEL_W, top, width - LABEL_W, LANE_H);

      ctx.fillStyle = "#141920";
      ctx.fillRect(0, top, LABEL_W, LANE_H);
      ctx.fillStyle = role === "mic" ? "#4ade80" : "#4ea1ff";
      ctx.font = "11px ui-sans-serif, system-ui, sans-serif";
      ctx.fillText(role, 8, mid);

      ctx.strokeStyle = "#232b36";
      ctx.beginPath();
      ctx.moveTo(0, top + 0.5);
      ctx.lineTo(width, top + 0.5);
      ctx.stroke();

      ctx.strokeStyle = role === "mic" ? "#4ade80" : "#4ea1ff";
      ctx.globalAlpha = 0.85;
      ctx.beginPath();
      for (let px = LABEL_W; px < width; px++) {
        const t0 = start + (px - LABEL_W) / pps;
        const t1 = start + (px + 1 - LABEL_W) / pps;
        let peak = 0;
        for (let b = Math.floor(t0 * rate); b <= Math.floor(t1 * rate); b++) {
          if (b >= 0 && b < data.length && data[b] > peak) peak = data[b];
        }
        const h = peak * (LANE_H / 2 - 3);
        ctx.moveTo(px + 0.5, mid - h);
        ctx.lineTo(px + 0.5, mid + h);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;
    });

    // marker lane
    ctx.fillStyle = "#141920";
    ctx.fillRect(0, RULER_H, width, MARKER_H);
    markers.forEach((m: Marker, i: number) => {
      const px = x(m.seconds);
      if (px < LABEL_W - 6 || px > width + 6) return;
      const colour = TAG_COLOUR[m.tag] ?? "#8a94a2";
      ctx.fillStyle = colour;
      ctx.globalAlpha = m.source === "gap-fill" ? 0.45 : 1;
      ctx.beginPath();
      ctx.moveTo(px, RULER_H + 3);
      ctx.lineTo(px + 5, RULER_H + 8);
      ctx.lineTo(px, RULER_H + 13);
      ctx.lineTo(px - 5, RULER_H + 8);
      ctx.closePath();
      ctx.fill();
      if (i === selected) {
        ctx.strokeStyle = "#ffcc4e";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
      ctx.globalAlpha = 0.25;
      ctx.strokeStyle = colour;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(Math.floor(px) + 0.5, RULER_H + MARKER_H);
      ctx.lineTo(Math.floor(px) + 0.5, height);
      ctx.stroke();
      ctx.globalAlpha = 1;
    });

    // playhead
    const ph = x(time);
    if (ph >= LABEL_W) {
      ctx.strokeStyle = "#f87171";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(Math.floor(ph) + 0.5, 0);
      ctx.lineTo(Math.floor(ph) + 0.5, height);
      ctx.stroke();
    }
  }, [duration, height, markers, peaks, roles, selected, time, zoom]);

  useEffect(() => {
    draw();
    const ro = new ResizeObserver(draw);
    if (wrapRef.current) ro.observe(wrapRef.current);
    return () => ro.disconnect();
  }, [draw]);

  const timeAt = (clientX: number): number => {
    const wrap = wrapRef.current!;
    const rect = wrap.getBoundingClientRect();
    const view = duration / zoom;
    const start = Math.max(0, Math.min(duration - view, time - view / 2));
    const pps = (rect.width - LABEL_W) / view;
    return Math.max(0, Math.min(duration, start + (clientX - rect.left - LABEL_W) / pps));
  };

  const onDown = (e: React.MouseEvent) => {
    const rect = wrapRef.current!.getBoundingClientRect();
    const y = e.clientY - rect.top;
    const t = timeAt(e.clientX);

    if (y >= RULER_H && y < RULER_H + MARKER_H) {
      const hit = markers.findIndex((m) => Math.abs(m.seconds - t) * (rect.width - LABEL_W) / (duration / zoom) < 7);
      if (hit >= 0) {
        select(hit);
        dragging.current = { kind: "marker", index: hit };
        return;
      }
    }
    dragging.current = { kind: "playhead" };
    setTime(t);
  };

  const onMove = (e: React.MouseEvent) => {
    if (!dragging.current) return;
    const t = timeAt(e.clientX);
    if (dragging.current.kind === "playhead") setTime(t);
    else if (dragging.current.index !== undefined) {
      updateMarker(dragging.current.index, { seconds: Math.round(t * 100) / 100 });
    }
  };

  const onWheel = (e: React.WheelEvent) => {
    if (!e.ctrlKey && !e.altKey) return;
    e.preventDefault();
    setZoom(zoom * (e.deltaY < 0 ? 1.25 : 0.8));
  };

  if (!current) return <div className="timeline empty" />;

  return (
    <div className="timeline">
      <div className="tl-bar">
        <span className="dim">timeline</span>
        <span className="spacer" />
        <span className="dim mono">{hms(time)} / {hms(duration)}</span>
        <div className="zoom">
          <button onClick={() => setZoom(zoom * 0.5)} title="Zoom out">−</button>
          <span className="mono dim">{zoom < 1 ? zoom.toFixed(2) : zoom.toFixed(0)}x</span>
          <button onClick={() => setZoom(zoom * 2)} title="Zoom in">+</button>
          <button onClick={() => setZoom(1)} title="Fit">fit</button>
        </div>
      </div>
      <div
        ref={wrapRef}
        className="tl-canvas"
        style={{ height }}
        onMouseDown={onDown}
        onMouseMove={onMove}
        onMouseUp={() => (dragging.current = null)}
        onMouseLeave={() => (dragging.current = null)}
        onWheel={onWheel}
      >
        <canvas ref={canvasRef} />
      </div>
    </div>
  );
}
