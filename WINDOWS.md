# Shipping chaptr on Windows

The target is one installer a non-technical person can run, followed by a
guided model download on first launch. Nothing in the app is Linux-specific —
paths already resolve `%APPDATA%`, executables already get a `.exe` suffix —
but none of it has been run on Windows yet.

## Two routes

### Cross-compile from this machine

Tauri v2 supports it through `cargo-xwin`:

```
cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
```

State of the toolchain here:

| Need | Status |
|---|---|
| clang | present |
| lld | present |
| llvm | present |
| `x86_64-pc-windows-msvc` std | **missing** — Arch's `rust` package ships no cross std |
| rustup | **missing** — installable to `~/.rustup` without root |
| cargo-xwin | **missing** — `cargo install cargo-xwin`, no root needed |
| makensis | **missing** — needs `pacman -S nsis`, i.e. **root** |

So without root we can likely produce a Windows **.exe** and confirm it
compiles and links. We cannot produce the **NSIS installer**; that needs
`makensis`. A portable zip is a reasonable fallback and still worth testing.

The thing cross-compiling can never tell us is whether the app actually *runs*:
whether the webview initialises, whether the sidecars find each other, whether
GPU acceleration works.

### Build and test on Windows

A VM, a spare machine, or a GitHub Actions `windows-latest` runner. CI is the
cheapest path to a *correct* installer, since it builds on a real Windows box
with WiX and NSIS already available, and needs no hardware here.

A local VM adds the ability to actually click the thing. GPU passthrough is not
realistic — this machine has a single 4090 in use by the host — so a VM would
be CPU-only. That is still enough to test the parts most likely to break:
installer, paths, sidecar discovery, model download, ffmpeg, the UI. It is not
enough to test Vulkan or measure speed.

## What must be bundled

| Piece | Source | Size | Notes |
|---|---|---|---|
| `whisper-cli.exe` | whisper.cpp releases | ~40 MB | use the **Vulkan** build |
| `llama-server.exe` | llama.cpp releases | ~50 MB | use the **Vulkan** build |
| `ffmpeg.exe` | gyan.dev or BtbN builds | ~80 MB | see licensing below |

Vulkan rather than CUDA: one binary covers NVIDIA, AMD and Intel, and there is
no CUDA runtime for the user to install. On a 5080 it is within roughly 15% of
CUDA for whisper, which is irrelevant next to not working at all.

These go in `bin/` next to the executable, which is already how `Sidecars`
finds them. Models land in `models/`, downloaded on first run.

Expected installer: 150-200 MB. First-run download: about 10 GB.

## Licensing

ffmpeg is the one to get right. An **LGPL** build can be bundled with
attribution and a copy of the licence. A **GPL** build would force chaptr
itself to be GPL. Pick the LGPL build deliberately rather than grabbing the
first zip.

whisper.cpp and llama.cpp are MIT. Model weights have their own terms — Qwen3
is Apache 2.0, which is fine for this.

## How it is built now

GitHub Actions, `.github/workflows/build.yml`. Run it from the Actions tab
("build" > Run workflow) or push a `v*` tag, which additionally opens a draft
release. Artifacts are kept for 14 days.

| Platform | Produces | Sidecars inside |
|---|---|---|
| windows-latest | NSIS installer `.exe` | ffmpeg, ffprobe, whisper-cli, llama-server |
| ubuntu-22.04 | `.AppImage` and `.deb` | ffmpeg, ffprobe only |

`scripts/fetch-sidecars.mjs` downloads them into `app/src-tauri/bin`, pinned to
exact release tags. `SIDECAR_TARGET=win32` runs the Windows set from Linux,
which is how the pinned URLs were checked. The bundler copies that folder to
`bin/` beside the installed binary.

Linux ships ffmpeg only: the whisper and llama Linux archives are dynamically
linked against their own `.so` files and would need rpath work to survive
packaging. Both fall back to PATH.

**whisper is CPU-only on Windows.** whisper.cpp publishes no Vulkan build, and
its CUDA builds are 12.4, which predates the 50-series. The BLAS build runs
anywhere and is far slower than the 120x measured locally with CUDA. Fixing
this means compiling whisper.cpp with Vulkan in CI.

## Still to do

- [ ] Build whisper.cpp with Vulkan in CI so Windows gets GPU transcription
- [ ] First-run screen: "chaptr needs to download about 10 GB"
- [ ] Verify `%APPDATA%\chaptr` paths on a real Windows box
- [ ] Confirm WebView2 is present or bootstrapped by the installer
- [ ] Test on a machine with no graphics card, since the app should still run
