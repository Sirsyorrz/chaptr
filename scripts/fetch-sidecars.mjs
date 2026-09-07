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

// whisper.cpp publishes no Vulkan build for Windows, so this is the CPU one.
// It works everywhere; it is not fast. See WINDOWS.md.
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
  // Linux ships ffmpeg only. The whisper and llama Linux archives are dynamically
  // linked against their own .so files, which needs rpath work an AppImage would
  // then have to preserve; both fall back to PATH instead.
  linux: [
    {
      name: "ffmpeg",
      url: `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-${FFMPEG}-linux64-lgpl-8.1.tar.xz`,
      pick: (f) => /[/\\]bin[/\\](ffmpeg|ffprobe)$/.test(f),
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
const wanted = sources[platform];
if (!wanted) throw new Error(`no sidecars defined for ${process.platform}`);

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
  for (const f of picked) copyFileSync(f, join(out, basename(f)));
  console.log(`  ${picked.length} files -> bin/`);
}

const required = platform === "win32"
  ? ["ffmpeg.exe", "ffprobe.exe", "whisper-cli.exe", "llama-server.exe"]
  : ["ffmpeg", "ffprobe"];
const absent = required.filter((f) => !existsSync(join(out, f)));
if (absent.length) throw new Error(`missing after fetch: ${absent.join(", ")}`);

const total = readdirSync(out).reduce((n, f) => n + statSync(join(out, f)).size, 0);
console.log(`\n${readdirSync(out).length} files, ${(total / 1e6).toFixed(0)} MB in ${out}`);
