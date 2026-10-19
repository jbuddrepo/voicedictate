# VoiceDictate Windows Beta Test Record

## 0.2.1 release validation

The historical results below describe earlier builds. The 0.2.1 changes add explicit Tap/Hold activation with 250 ms Hold release grace, serialized shortcut events, cleanup diagnostics, and a synthetic cleanup-response test. Automated regression tests cover shortcut ordering, old-settings migration, HTTP cleanup requests, empty/error responses, and raw-text fallback.

Before calling the laptop fully verified, test Tap and Hold modes with real speech, compare final words three times with English selected, check Notepad insertion and clipboard restoration, and run Test cleanup against the actual configured server. These hands-on checks have not been completed by the automated tests. No user recording or dictated text is retained for diagnostics.

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
- [ ] Test AI cleanup from the installed app with the configured Ollama server
- [ ] Disconnect the cleanup server and confirm the raw transcript is pasted unchanged
- [ ] Test a whisper.cpp server over LAN or Tailscale
- [ ] Test an OpenAI-compatible `/v1/audio/transcriptions` provider
- [ ] Disconnect the remote server and confirm Automatic mode uses local Whisper
- [ ] Confirm Remote mode reports an actionable error instead of silently sending elsewhere
- [ ] Fresh-machine installer and portable ZIP smoke tests

## Open findings for v0.2.1

### AI cleanup connection failed in the installed app

The user reported a connection failure when testing AI cleanup. The earlier command-line cleanup test passed, but that does not prove the installed application's saved URL, model, request format, timeout, and fallback behavior work together.

Before the next build, test the complete flow from VoiceDictate using the configured Ollama server and model. Verify the connection-test message is accurate, cleaned text is returned when the server is available, and a failed or timed-out cleanup request still pastes the raw transcript without losing the dictation. Do not log or retain dictated text while diagnosing the connection.

### Shortcut press duration is ambiguous

The current shortcut combines tap-to-toggle and hold-to-talk using a fixed 350 ms cutoff. A press shorter than 350 ms starts toggle recording; a press held for 350 ms or longer stops recording as soon as the keys are released. A normal keypress can therefore be interpreted as hold-to-talk and stop earlier than the user expects.

Recommended behavior: add an explicit **Activation behavior** setting with **Tap to start/stop** as the default and **Hold to talk** as the optional mode. Avoid exposing a timing slider unless testing shows it is still necessary. Hold mode should include a short release grace period so the final spoken sound is not clipped.

### Inconsistent final words

The user reported inconsistent recognition near the end of a dictation, with phrases resembling “said by default” being transcribed differently. Possible causes include the shortcut release ending capture too quickly, automatic language detection on a short phrase, microphone clarity, or the selected Whisper model.

Retest after activation modes and the release grace period are implemented. Compare the same phrase at least three times with Language set first to **Auto detect** and then to **English**. Keep audio deletion as the default; any temporary diagnostic-audio retention must require explicit user consent.
