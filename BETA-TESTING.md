# VoiceDictate Windows Beta Test Record

## Reference machine

- Windows 11 25H2 x64, build 26200
- Intel Core i5-3210M, 2 cores / 4 threads
- 15.8 GB RAM
- Intel HD Graphics 4000 (not used)
- AVX supported; AVX2 and FMA unavailable

## Automated results

- React/TypeScript production build: passed
- Rust desktop backend compile: passed
- Native whisper.cpp worker compile: passed
- Backend unit tests: 3 passed, 0 failed
- Tiny Q5 multilingual model checksum and load: passed
- 11-second official JFK sample: accurate transcript, 22.7 seconds wall time
- 5-second sample: transcript completed in 10.0 seconds
- Remote Tailscale Ollama model discovery: passed
- Remote `qwen2.5:3b-instruct` cleanup request: passed
- Release executable launch and responsive-process smoke test: passed
- Startup data store creation with zero temporary audio files: passed
- NSIS installer, portable ZIP, checksums, and updater signature: passed

The five-second dictation does not meet the original five-second latency target on this 2012 CPU. VoiceDictate remains usable, but this is a known beta performance limitation. Newer AVX2/FMA computers automatically receive the optimized worker.

Interactive UI automation could not continue because Windows had an unrelated CMake-installer UAC prompt waiting for the user. VoiceDictate itself launched and remained responsive; the security prompt was intentionally not dismissed automatically.

## Manual release checklist

- [ ] Tap-to-toggle and hold-to-talk with the configured global shortcut
- [ ] Paste into Notepad, Chrome, Word, VS Code, and Windows Terminal
- [ ] Unicode, multiline text, and clipboard restoration
- [ ] Microphone denial, silence, and device-disconnect messages
- [ ] Cancel, rapid reactivation, sleep, and lock/unlock
- [ ] Search and completely clear text-only history
- [ ] Confirm no audio files remain after success or failure
- [ ] Confirm no network traffic when transcription is Local and cleanup is disabled
- [ ] Test a whisper.cpp server over LAN or Tailscale
- [ ] Test an OpenAI-compatible `/v1/audio/transcriptions` provider
- [ ] Disconnect the remote server and confirm Automatic mode uses local Whisper
- [ ] Confirm Remote mode reports an actionable error instead of silently sending elsewhere
- [ ] Fresh-machine installer and portable ZIP smoke tests
