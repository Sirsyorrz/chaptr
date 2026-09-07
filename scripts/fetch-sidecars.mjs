#!/usr/bin/env node
// Downloads the executables chaptr drives into app/src-tauri/bin, where the
// bundler picks them up and Sidecars::discover finds them at runtime.
//
// Versions are pinned: a moving "latest" would silently change what ships.

import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, existsSync, copyFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "app", "src-tauri", "bin");
const cache = join(root, ".sidecar-cache");

const WHISPER = "b4938";
const LLAMA = "b10830";
const FFMPEG = "n8.1-latest";

// whisper.cpp publishes no Vulkan build for any platform, so the entries below
// are the CPU ones. scripts/build-whisper-vulkan.mjs compiles a GPU whisper-cli
// instead, and is run with SIDECAR_SKIP=whisper set here. See WINDOWS.md.
const sources = {
  win32: [
    {
      name: "ffmpeg",
      url: `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-${FFMPEG}-win64-lgpl-shared-8.1.zip`,
      pick: (f) => /[/\\]bin[/\\].+\.(exe|dll)$/i.test(f) && !/ffplay/i.test(f),
    },
    {
      name: "whisper",
      url: `https://github.com/ggml-org/whisper.cpp/releases/download/${WHISPER}/whisper-blas-bin-x64.zip`,
      pick: (f) => /whisper-cli\.exe$/i.test(f) || /\.dll$/i.test(f),
    },
    {
      name: "llama",
      url: `https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA}/llama-${LLAMA}-bin-win-vulkan-x64.zip`,
      pick: (f) => /llama-server\.exe$/i.test(f) || /\.dll$/i.test(f),
    },
  ],
  // The Linux whisper and llama builds are dynamically linked against the .so
  // files packed beside them, so those ship too and sidecar::command points the
  // loader at the folder.
  linux: [
    {
      name: "ffmpeg",
      url: `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-${FFMPEG}-linux64-lgpl-8.1.tar.xz`,
      pick: (f) => /[/\\]bin[/\\](ffmpeg|ffprobe)$/.test(f),
    },
    {
      name: "whisper",
      url: `https://github.com/ggml-org/whisper.cpp/releases/download/${WHISPER}/whisper-bin-ubuntu-x64.tar.gz`,
      pick: (f) => /[/\\]whisper-cli$/.test(f) || /\.so(\.\d+)*$/.test(f),
    },
    {
      name: "llama",
      url: `https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA}/llama-${LLAMA}-bin-ubuntu-vulkan-x64.tar.gz`,
      pick: (f) => /[/\\]llama-server$/.test(f) || /\.so(\.\d+)*$/.test(f),
    },
  ],
};

const run = (cmd, args, cwd) =>
  execFileSync(cmd, args, { cwd, stdio: ["ignore", "pipe", "inherit"] }).toString();

function download(url, dest) {
  if (existsSync(dest)) {
    console.log(`  cached ${basename(dest)}`);
    return;
  }
  console.log(`  fetching ${basename(dest)}`);
  run("curl", ["-sSL", "--fail", "-o", dest, url]);
}

function extract(archive, dir) {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  // Windows ships bsdtar, which reads zip. GNU tar does not, so on Linux a zip
  // goes through unzip - only reached when testing the Windows set from here.
  if (archive.endsWith(".zip") && process.platform !== "win32") {
    run("unzip", ["-q", "-o", archive, "-d", dir]);
  } else {
    run("tar", ["-xf", archive], dir);
  }
}

function walk(dir) {
  return readdirSync(dir).flatMap((e) => {
    const p = join(dir, e);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

// SIDECAR_TARGET exists so the Windows set can be exercised from a Linux box.
const platform = process.env.SIDECAR_TARGET || (process.platform === "win32" ? "win32" : "linux");
const skip = new Set((process.env.SIDECAR_SKIP || "").split(",").filter(Boolean));
const all_sources = sources[platform];
if (!all_sources) throw new Error(`no sidecars defined for ${process.platform}`);
const wanted = all_sources.filter((s) => !skip.has(s.name));

mkdirSync(out, { recursive: true });
mkdirSync(cache, { recursive: true });

for (const src of wanted) {
  console.log(src.name);
  const archive = join(cache, basename(new URL(src.url).pathname));
  download(src.url, archive);

  const staging = join(cache, src.name);
  extract(archive, staging);

  const picked = walk(staging).filter(src.pick);
  if (!picked.length) throw new Error(`${src.name}: archive contained nothing matching`);

  // Each tool gets its own folder. whisper.cpp and llama.cpp both ship
  // libggml-base and friends built from different revisions; flattened
  // together, whichever landed last would be loaded by both.
  const dest = join(out, src.name);
  mkdirSync(dest, { recursive: true });
  for (const f of picked) copyFileSync(f, join(dest, basename(f)));
  console.log(`  ${picked.length} files -> bin/${src.name}/`);
}

const suffix = platform === "win32" ? ".exe" : "";
const required = [
  ["ffmpeg", `ffmpeg${suffix}`],
  ["ffmpeg", `ffprobe${suffix}`],
  ["whisper", `whisper-cli${suffix}`],
  ["llama", `llama-server${suffix}`],
];
const absent = required
  .filter(([d]) => !skip.has(d))
  .filter(([d, f]) => !existsSync(join(out, d, f)));
if (absent.length) {
  throw new Error(`missing after fetch: ${absent.map(([d, f]) => `${d}/${f}`).join(", ")}`);
}

const all = walk(out);
const total = all.reduce((n, f) => n + statSync(f).size, 0);
console.log(`\n${all.length} files, ${(total / 1e6).toFixed(0)} MB in ${out}`);
