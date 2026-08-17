# VoiceDictate

VoiceDictate is a free, open-source Windows dictation app. Press a shortcut, speak, and have Whisper transcription pasted into any application. Local transcription works without an account, subscription, word limit, telemetry, or required server. Users can optionally configure their own private or hosted transcription endpoint.

## Download for Windows

### [Download the VoiceDictate installer](https://github.com/jbuddrepo/voicedictate/releases/download/0.2.0-beta.1/VoiceDictate-Setup-0.2.0.exe)

### [Download the portable ZIP](https://github.com/jbuddrepo/voicedictate/releases/download/0.2.0-beta.1/VoiceDictate-Portable-0.2.0.zip)

The portable version does not need installation: extract the whole ZIP and run VoiceDictate. This is an unsigned beta, so Windows SmartScreen may show a warning. See the [releases page](https://github.com/jbuddrepo/voicedictate/releases) for checksums, release notes, and future versions.

Version 0.2.0 adds user-configured whisper.cpp and OpenAI-compatible transcription servers, Automatic local fallback, and the persistent local worker introduced during 0.1.1 development.

## Guides

- [Set up Whisper on a K11, Linux VM, home server, LAN, or Tailscale](REMOTE-TRANSCRIPTION.md)
- [Build VoiceDictate and a native whisper.cpp server](BUILDING.md)
- [Beta testing checklist](BETA-TESTING.md)

## Privacy model

- Microphone audio is transcribed locally by default and deleted immediately.
- History stores text only in the current Windows user's application-data folder.
- No network requests occur unless an optional model download, remote transcription, or AI cleanup is enabled.
- Remote transcription sends the temporary recording only to the endpoint explicitly configured by the user.
- AI cleanup sends only transcript text to the Ollama or OpenAI-compatible endpoint configured by the user.

See [PRIVACY.md](PRIVACY.md) for the complete behavior.

## Windows beta features

- Tap-to-toggle and hold-to-talk global shortcut
- Bundled CPU-optimized multilingual Whisper Tiny Q5 model
- Optional multilingual Base model
- Optional whisper.cpp and OpenAI-compatible remote transcription
- Automatic remote transcription with local Whisper fallback
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

Run `scripts\build.ps1`. The script builds the worker, generates icons, builds the application, and places release files under `artifacts\`. See [BUILDING.md](BUILDING.md) for the complete build and verification process.

Tagged GitHub builds can produce signed updater artifacts when the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are configured. The matching public key is committed in `UPDATER-PUBLIC-KEY.txt`; the private key must never be committed.

## Development

```powershell
pnpm install
pnpm tauri:dev
```

The Whisper model is intentionally excluded from Git. `scripts\fetch-models.ps1` downloads the official whisper.cpp conversion and verifies its SHA-256 digest against the published manifest.

## License

VoiceDictate is licensed under Apache-2.0. whisper.cpp is MIT licensed. Model and third-party notices are in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
