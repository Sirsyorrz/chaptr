# chaptr

Point it at gameplay recordings. It transcribes them locally and writes a
searchable, timestamped play-by-play — so you can find the moments worth
cutting without scrubbing a hundred hours by hand.

Everything runs on your machine. No uploads, no account, no API key.

## What it does

1. **Scan** a folder of recordings
2. **Transcribe** them with whisper.cpp
3. **Summarise** the transcript into chaptrs — one line per moment, timestamped
4. **Search and star** the ones worth keeping
5. **Click one** and DaVinci Resolve jumps to that frame

A hundred hours takes roughly two hours unattended, and every stage resumes, so
stopping partway costs nothing.

## Install

Grab the latest [release](https://github.com/Sirsyorrz/chaptr/releases):

- **Windows** — `chaptr_x.y.z_x64-setup.exe`
- **Linux** — `.AppImage` or `.deb`

ffmpeg, whisper.cpp and llama.cpp are bundled. The app updates itself when a
new release is published.

On first run open **Settings** and download a speech model and a language
model, around 10 GB depending on the tier you pick.

## Requirements

- Windows 11 or Linux
- A GPU with 8 GB of VRAM or more for the larger language models
- DaVinci Resolve Studio, for playhead seeking

## Seeking in Resolve

**Settings → DaVinci Resolve → Install script**, then in Resolve run
**Workspace → Scripts → Chaptr Link** and leave it running. Clicking a chaptr or
a transcript line moves the playhead.

Resolve's API can set the playhead but cannot start or stop playback, so press
play in Resolve yourself.

## Building

```sh
node scripts/fetch-sidecars.mjs   # ffmpeg, whisper, llama into app/src-tauri/bin
cd app
npm install
npm run tauri build
```

`chaptr-cli` runs the same pipeline headless, which is the sane way to drive the
long stages:

```sh
chaptr-cli scan|transcribe|chaptrs|projects|resolve|models|doctor
```

Windows GPU transcription is compiled in CI by
`scripts/build-whisper-vulkan.mjs`, because whisper.cpp publishes no Vulkan
binaries.

## Status

Early. Windows support is new and lightly tested. Linux transcription in the
packaged builds is CPU-only for now; Windows uses Vulkan.
