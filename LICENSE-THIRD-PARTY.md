# Third-party components

chaptr ships three programs it did not write. They stay separate executables,
invoked as child processes; chaptr links against none of them.

## FFmpeg — LGPL v2.1 or later

Used to read footage and extract audio. The bundled build is compiled with
`--disable-gpl` and contains no GPL components.

- Source and licence: <https://ffmpeg.org>
- Build used: BtbN FFmpeg-Builds, `win64-lgpl-shared` / `linux64-lgpl`
- Build scripts: <https://github.com/BtbN/FFmpeg-Builds>

The LGPL requires that you be able to replace this component. The binaries sit
in the `bin` folder beside the application and may be swapped for another
LGPL-compatible FFmpeg build of the same version.

## whisper.cpp — MIT

Speech recognition. <https://github.com/ggml-org/whisper.cpp>

## llama.cpp — MIT

Runs the local language model. <https://github.com/ggml-org/llama.cpp>

## Models

Downloaded on first run, not bundled, each under its own terms:

- Whisper models (OpenAI) — MIT
- Silero VAD — MIT
- Qwen3 (Alibaba) — Apache 2.0
