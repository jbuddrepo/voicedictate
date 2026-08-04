# Third-Party Notices

VoiceDictate depends on open-source components. Release builds must include their complete license texts.

- **whisper.cpp** — MIT License, Copyright the ggml authors.
- **Whisper model weights** — distributed by OpenAI and converted for whisper.cpp; see the upstream Whisper repository and model card.
- **Tauri** — Apache-2.0 and MIT licenses.
- **React** — MIT License.
- **Rust crates and JavaScript packages** — individual licenses recorded in `Cargo.lock` and `pnpm-lock.yaml`.

The release pipeline must generate a dependency-license inventory before publishing a stable build.
