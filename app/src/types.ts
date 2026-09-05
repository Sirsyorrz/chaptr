export type Tag =
  | "combat" | "objective" | "banter" | "planning"
  | "highlight" | "death" | "downtime" | "meta";

export const TAGS: Tag[] = [
  "combat", "objective", "banter", "planning",
  "highlight", "death", "downtime", "meta",
];

export interface Marker {
  seconds: number;
  t: string;
  title: string;
  note: string;
  tag: Tag;
  speakers: string[];
  source: "llm" | "gap-fill" | "manual";
  confidence: number;
}

export interface Utterance {
  start: number;
  end: number;
  speaker: string;
  text: string;
  track: string;
}

export interface ClipSummary {
  key: string;
  name: string;
  duration: number;
  fps: number;
  wall_start: string;
  session_id: number;
  markers: number;
  has_proxy: boolean;
  edited: boolean;
}

export interface Peaks {
  rate: number;
  tracks: Record<string, number[]>;
}

export interface ClipDetail {
  key: string;
  clip: {
    clip: string;
    duration: number;
    fps: number;
    timeline: Utterance[];
  };
  markers: Marker[];
  peaks: Peaks | null;
  proxy: string | null;
  edited: boolean;
}

export const hms = (s: number): string => {
  const v = Math.max(0, Math.floor(s));
  const h = Math.floor(v / 3600);
  const m = Math.floor((v % 3600) / 60);
  const sec = v % 60;
  return [h, m, sec].map((n) => String(n).padStart(2, "0")).join(":");
};

export const hmsf = (s: number, fps: number): string =>
  `${hms(s)}:${String(Math.floor((s % 1) * fps)).padStart(2, "0")}`;

export const parseHms = (t: string): number | null => {
  const parts = t.split(":").map(Number);
  if (parts.some(isNaN) || parts.length === 0) return null;
  while (parts.length < 3) parts.unshift(0);
  return parts[0] * 3600 + parts[1] * 60 + parts[2];
};
