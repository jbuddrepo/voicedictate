# VoiceDictate

VoiceDictate is a free, open-source Windows dictation app. Press a shortcut, speak, and have local Whisper transcription pasted into any application. It has no account, subscription, word limit, telemetry, or required server.

## Download for Windows

### [Download the VoiceDictate installer](https://github.com/jbuddrepo/voicedictate/releases/download/0.1.0-beta.1/VoiceDictate-Setup-0.1.0.exe)

### [Download the portable ZIP](https://github.com/jbuddrepo/voicedictate/releases/download/0.1.0-beta.1/VoiceDictate-Portable-0.1.0.zip)

The portable version does not need installation: extract the whole ZIP and run VoiceDictate. This is an unsigned beta, so Windows SmartScreen may show a warning. See the [releases page](https://github.com/jbuddrepo/voicedictate/releases) for checksums, release notes, and future versions.

## Privacy model

- Microphone audio is transcribed locally and deleted immediately.
- History stores text only in the current Windows user's application-data folder.
- No network requests occur unless optional AI cleanup or an optional model download is enabled.
- AI cleanup sends only transcript text to the Ollama or OpenAI-compatible endpoint configured by the user.

See [PRIVACY.md](PRIVACY.md) for the complete behavior.

## Windows beta features

- Tap-to-toggle and hold-to-talk global shortcut
- Bundled CPU-optimized multilingual Whisper Tiny Q5 model
- Optional multilingual Base model
- Automatic text insertion and clipboard restoration
- Text-only searchable history
- Optional Ollama and OpenAI-compatible cleanup
- Baseline AVX support for older processors and AVX2 builds for modern processors

## Build prerequisites

- Windows 10 or 11 x64
- Visual Studio 2022 Build Tools with the Desktop C++ workload
- Stable Rust MSVC toolchain
- Node.js LTS and pnpm
- CMake and Ninja
- LLVM/libclang (used to generate Windows bindings for whisper.cpp)

Run `scripts\build.ps1`. The script builds the worker, generates icons, builds the application, and places release files under `artifacts\`.

Tagged GitHub builds can produce signed updater artifacts when the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are configured. The matching public key is committed in `UPDATER-PUBLIC-KEY.txt`; the private key must never be committed.

## Development

```powershell
pnpm install
pnpm tauri:dev
```

The Whisper model is intentionally excluded from Git. `scripts\fetch-models.ps1` downloads the official whisper.cpp conversion and verifies its SHA-256 digest against the published manifest.

## License

VoiceDictate is licensed under Apache-2.0. whisper.cpp is MIT licensed. Model and third-party notices are in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
