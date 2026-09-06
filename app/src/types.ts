export type Role = "mixed" | "game" | "mic" | "voice" | "ignore" | "unknown";

export const ROLES: Role[] = ["mixed", "game", "mic", "voice", "ignore", "unknown"];

/** Wording the user recognises; the stored values stay as they are. */
export const ROLE_LABEL: Record<Role, string> = {
  mixed: "everything mixed",
  game: "game audio",
  mic: "me (my mic)",
  voice: "friends (Discord)",
  ignore: "ignore",
  unknown: "not sure",
};

export const ROLE_HELP: Record<Role, string> = {
  mixed: "Game, your mic and your friends all in one track",
  game: "Game and system sound only. Never transcribed — it just produces gibberish",
  mic: "Only you, the person who recorded this",
  voice: "Only the other people in the call, e.g. Discord",
  ignore: "Skip this track entirely",
  unknown: "Not identified. Pick what it is",
};

export interface Track {
  index: number;
  name: string;
  codec: string;
  channels: number;
}

export interface Recording {
  id: string;
  path: string;
  name: string;
  duration: number;
  wall_start: string;
  stamp_source: string;
  session_id: number;
  session_offset: number;
  global_offset: number;
  tracks: Track[];
}

export interface Session {
  id: number;
  label: string;
  start: string;
  duration: number;
  global_offset: number;
  recordings: string[];
}

export interface Project {
  id: string;
  name: string;
  sources: string[];
  created: string;
  opened: string;
  recordings: number;
  duration: number;
  file: string | null;
  unsaved: boolean;
}

export interface Library {
  root: string;
  scanned_at: string;
  total_duration: number;
  sessions: Session[];
  recordings: Recording[];
}

export interface Chaptr {
  global: number;
  recording_id: string;
  offset: number;
  text: string;
  starred: boolean;
  source: string;
}

export interface Segment {
  start: number;
  end: number;
  text: string;
  who: string;
}

export interface Transcript {
  recording_id: string;
  duration: number;
  model: string;
  segments: Segment[];
}

export interface TrackProbe {
  index: number;
  name: string;
  channels: number;
  words_per_minute: number;
  sample: string;
  role: Role;
  confident: boolean;
}

export interface Layout {
  signature: string;
  track_count: number;
  recordings: number;
  example: string;
  tracks: TrackProbe[];
}

export interface Settings {
  subject: string;
  notes: string;
  roles: Record<string, Role[]>;
}

/** Hours are what the user thinks in across a 100 hour shoot. */
export const hms = (s: number): string => {
  const v = Math.max(0, Math.floor(s));
  const h = Math.floor(v / 3600);
  const m = Math.floor((v % 3600) / 60);
  const sec = v % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h ? `${h}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
};

export const hoursMins = (s: number): string => {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h ? `${h}h ${m}m` : `${m}m`;
};

export interface JobProgress {
  run: number;
  stage: string;
  index: number;
  total: number;
  name: string;
  fraction: number;
  done: boolean;
  cancelled: boolean;
  message: string;
  error: string | null;
}

export interface FileStatus {
  id: string;
  name: string;
  duration: number;
  global_offset: number;
  session_id: number;
  segments: number;
  chaptrs: number;
  transcribed: boolean;
}
