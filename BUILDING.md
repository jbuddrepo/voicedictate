# Building VoiceDictate

## Supported build target

The current beta targets Windows 10/11 x64. The installed application is self-contained; these developer tools are needed only to build it.

## Prerequisites

- Visual Studio 2022 Build Tools with the Desktop development with C++ workload
- Stable Rust MSVC toolchain
- Node.js LTS and pnpm
- CMake and Ninja
- LLVM/libclang, either installed under `Program Files\LLVM` or placed under `work\libclang\runtimes\win-x64\native`

## Build from source

1. Clone the repository and enter its directory.
2. Confirm the tools are available:

   ```powershell
   rustc --version
   cargo --version
   node --version
   pnpm --version
   cmake --version
   ninja --version
   ```

3. Build a development version and run all desktop tests:

   ```powershell
   .\scripts\build.ps1 -Configuration Debug
   ```

4. Build the release installer and portable ZIP:

   ```powershell
   .\scripts\build.ps1 -Configuration Release
   ```

5. Find the installer, portable ZIP, updater signature when configured, and `SHA256SUMS.txt` under `artifacts\`.

The build script downloads and verifies the bundled multilingual Tiny model when it is absent. It separately compiles a baseline AVX worker and an AVX2/FMA worker, then lets VoiceDictate select the correct executable at runtime.

## Updater signing

Updater artifact signing is separate from Windows Authenticode signing. Set `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` before the Release build. Never commit a private signing key or password. Unsigned beta executables can still trigger Windows SmartScreen.

## Remote-transcription implementation contract

VoiceDictate supports:

- A whisper.cpp server accepting multipart WAV uploads at `/inference` and returning JSON containing `text`.
- An OpenAI-compatible server accepting multipart WAV uploads at `/v1/audio/transcriptions` and returning JSON containing `text`.

The client sends a 16 kHz mono WAV, `response_format=json`, an optional language, and a model field for OpenAI-compatible providers. Optional credentials are sent as a Bearer token and stored with Windows Credential Manager.

Public URLs must use HTTPS. Plain HTTP is limited to loopback, private IPv4/IPv6, link-local, and Tailscale CGNAT addresses. HTTP redirects are disabled so an approved private URL cannot silently redirect audio elsewhere.

Version 0.2.2 can submit completed pause-delimited passages during capture through these same multipart WAV endpoints. This overlaps work with speaking; it is not a separate provider-specific real-time protocol. Automatic mode switches remaining passages to the isolated local worker after the first server failure. A temporary passage WAV is retained only for its request/inference and deleted afterward. Cancellation stops further passages and prevents final paste; an already submitted blocking operation may finish before its temporary file can be removed.

Both worker CMake hooks include `scripts/cmake/release-optimization.cmake`. This preserves CPU instruction gates and explicitly enables `/O2` and `NDEBUG` for MSVC Release/RelWithDebInfo configurations. Inspect generated compiler commands when changing build wrappers; a Rust release profile alone does not prove that the native C++ inference core is optimized.

## Build a native whisper.cpp server without Docker

Docker is recommended for reproducible deployment, but it is not required. On an Ubuntu/Debian server:

```bash
sudo apt update
sudo apt install -y build-essential cmake git
git clone --depth 1 https://github.com/ggml-org/whisper.cpp.git
cd whisper.cpp
sh ./models/download-ggml-model.sh base
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release -j "$(nproc)"
./build/bin/whisper-server --host 0.0.0.0 --port 8080 -m ./models/ggml-base.bin -t 8
```

Run it initially in the foreground and test it before creating a systemd service. Bind access to a trusted LAN or Tailscale interface and follow every warning in [REMOTE-TRANSCRIPTION.md](REMOTE-TRANSCRIPTION.md).

## Verification before release

```powershell
cargo test --offline --manifest-path .\src-tauri\Cargo.toml
pnpm build
```

Then test Local, Remote, and Automatic modes. Confirm that Automatic mode falls back when the remote server is stopped, Remote mode reports a useful error, history remains text-only, and Local mode plus disabled cleanup generates no network traffic.

