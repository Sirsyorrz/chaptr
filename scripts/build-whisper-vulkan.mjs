#!/usr/bin/env node
// Builds whisper-cli with the Vulkan backend into app/src-tauri/bin/whisper.
//
// whisper.cpp publishes CPU and CUDA binaries only, so a GPU build that works
// on NVIDIA, AMD and Intel alike has to be compiled here. Needs a Vulkan SDK
// (glslc + headers) on PATH or in VULKAN_SDK; CI installs one.
//
// Keep WHISPER in step with fetch-sidecars.mjs.

import { execFileSync } from "node:child_process";
import { mkdirSync, existsSync, copyFileSync, readdirSync, statSync, rmSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { cpus } from "node:os";
import { fileURLToPath } from "node:url";

const WHISPER = "b4938";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cache = join(root, ".sidecar-cache");
// Source and object tree deliberately sit outside .sidecar-cache: only the
// handful of staged artefacts are worth carrying between CI runs.
const work = join(root, ".whisper-build");
const src = join(work, WHISPER);
const build = join(src, "build");
const staged = join(cache, `whisper-vulkan-${WHISPER}`);
const out = join(root, "app", "src-tauri", "bin", "whisper");

const win = process.platform === "win32";
const exe = win ? ".exe" : "";
const lib = win ? /\.dll$/i : /\.so(\.\d+)*$/;

const run = (cmd, args, cwd) => execFileSync(cmd, args, { cwd, stdio: "inherit" });

function walk(dir) {
  return readdirSync(dir).flatMap((e) => {
    const p = join(dir, e);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

function compile() {
  if (!existsSync(join(src, "CMakeLists.txt"))) {
    rmSync(src, { recursive: true, force: true });
    mkdirSync(work, { recursive: true });
    run("git", ["clone", "--depth", "1", "--branch", WHISPER,
      "https://github.com/ggml-org/whisper.cpp", src], work);
  }

  run("cmake", ["-B", build, "-S", src,
    "-DCMAKE_BUILD_TYPE=Release",
    "-DGGML_VULKAN=ON",
    // The CPU backend still runs the parts Vulkan does not, and -march=native
    // would pin the shipped binary to whatever ISA the build machine had.
    "-DGGML_NATIVE=OFF",
    "-DBUILD_SHARED_LIBS=ON",
    "-DWHISPER_BUILD_TESTS=OFF",
    "-DWHISPER_BUILD_SERVER=OFF",
    "-DWHISPER_BUILD_EXAMPLES=ON",
  ], root);
  run("cmake", ["--build", build, "--config", "Release", "--target", "whisper-cli",
    "-j", String(cpus().length)], root);

  // Multi-config generators bury the artefacts one level deeper, so just take
  // whatever the tree holds rather than guessing the layout.
  const picked = walk(build).filter(
    (f) => basename(f) === `whisper-cli${exe}` || lib.test(f),
  );
  if (!picked.some((f) => basename(f) === `whisper-cli${exe}`)) {
    throw new Error("build produced no whisper-cli");
  }

  rmSync(staged, { recursive: true, force: true });
  mkdirSync(staged, { recursive: true });
  for (const f of picked) copyFileSync(f, join(staged, basename(f)));
}

mkdirSync(cache, { recursive: true });
if (existsSync(join(staged, `whisper-cli${exe}`))) {
  console.log(`cached whisper ${WHISPER} vulkan build`);
} else {
  compile();
}

// Replaced wholesale: the CPU build ships BLAS libraries the Vulkan one does
// not, and a stale ggml-blas beside a Vulkan ggml is a loader coin toss.
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const files = readdirSync(staged);
for (const f of files) copyFileSync(join(staged, f), join(out, f));
console.log(`${files.length} files -> bin/whisper/ (vulkan)`);
