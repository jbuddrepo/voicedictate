# VoiceDictate changes

The user authorized implementation of the recording/transcription, status-window, duplicate-instance and shortcut fixes, with offline transcription retained as the default, and requested a GitHub update. The original comparison used the 0.2.1 source and installed Wispr Flow 1.6.1264; see [comparison](WISPR-FLOW-COMPARISON.md). This document distinguishes implementation from target-PC verification.

## Implemented for 0.2.2

0.2.3 hotfix: defer framework-created windows until all app state is registered, fixing the work-PC `state not managed` startup error. Update the GitHub build actions to Node.js 24 runtimes. Target-PC verification of this hotfix remains necessary.

- [x] Start recognition on completed passages during natural pauses while recording continues. Preserve continuous speech until a safe pause or final pass; finish the suffix, perform optional cleanup once, and paste the complete text once. Failed speculative passes do not advance the audio cursor. Cancellation cannot consume a newer recording or paste partial output.
- [x] Keep Local mode offline by default. Remote/Automatic can submit pause-delimited WAV passages through their existing configured endpoints; Automatic switches remaining passages to local after a remote failure. No provider-specific real-time socket was added.
- [x] Reduce unnecessary audio copying with metadata/count polling; expose source, processing timings, thread count and passages processed during speech.
- [x] Explicitly optimize the release native inference core, retaining baseline AVX and separate AVX2/FMA variants. Increase automatic thread selection conservatively to at most eight, leaving a logical processor available where possible.
- [x] Make status backend-owned, queue window operations on the UI thread without holding state mutexes, replay state after renderer readiness, and reject stale revisions. Place the pop-up in the cursor display's work area, show without taking focus, retain success/error feedback briefly, and correct Hold instructions.
- [x] Prevent duplicate updated instances and bring the existing main window forward. Use isolated audio directories and conservative orphan cleanup instead of deleting another process's active recordings.
- [x] Support Ctrl + Windows alone and press-to-select shortcut capture. Pause dictation shortcuts during capture, support Escape cancellation, and apply/validate saved settings without re-register races.
- [x] Refresh microphones without restarting, show the saved active method, and collapse disabled cleanup controls.
- [x] Migrate existing text history while adding actual transcription source and processing time. Recording duration remains separate from processing time.

## Target-PC validation still required

- [ ] Measure the work laptop's stop-to-text delay with one main instance; record version, mode, model, language, cleanup settings and CPU/worker details. Implementation and synthetic tests do not prove the reported lag is resolved.
- [ ] Confirm the bottom pop-up during real Tap/Hold use on all attached monitors and DPI/taskbar configurations.
- [ ] Confirm Ctrl + Windows alone in normal applications, microphone disconnect/reconnect, final-word recognition, and paste/clipboard behavior.
- [ ] Verify the user's actual cleanup/transcription server and failure fallback. No live account/server configuration was changed for testing.
- [ ] Fresh-PC installation and normal daily-use regression review before treating the beta as production-ready.

## Implemented for 0.2.4

0.2.5 follow-up: pair native show and hide operations so the pop-up actually disappears despite Tauri's unchanged visibility cache. Add direct tray-click restore and an explicit Settings menu item. The near-clock tray icon remains available while the dictation pop-up hides.

The user authorized these next-build features on October 28, 2026.

- [x] **Pop-up lifetime.** Show while recording, then hide after 1.2 seconds. Transcription/cleanup can continue in the background; later terminal statuses do not reopen it.
- [x] **Movable pop-up.** Drag the pop-up to move it. Save the position locally and clamp it into a monitor work area after display changes.
- [x] **Smaller pop-up.** Reduce dimensions from 280×76 to 210×57 (75%), with compact status text and a transparent overlay document.
- [x] **System tray.** Keep Open and Quit near the Windows clock and omit the main window's taskbar button, including while the main window is open. Closing the main window continues to hide it.
- [x] **Personal/job vocabulary.** Add/edit/remove/enable locally stored preferred spellings and explicit approved mishearings. Supply bounded hints only to local Whisper; correct complete words/phrases locally on the final result after optional cleanup. Preserve original recognition in history. Reject conflicting enabled rules. Remote requests do not include glossary lists or aliases; cleanup receives the original recognized transcript. Recognition hints cannot guarantee spelling and cleanup paraphrasing can still prevent an exact alias match.

Target-PC validation: confirm drag, automatic hiding, saved position, monitor/DPI changes, tray access, and actual job vocabulary with short/long dictation. Local tests do not replace these checks.

## Later backlog

- [ ] Functional Start with Windows UI/registration (currently only a saved model field).
- [ ] History retention and disable-history options.
- [ ] Settings column spacing and an explicitly selected visual direction/mockups for broader Dictate/Settings redesign.
- [ ] Keep macOS/Linux expansion separate from Windows reliability work.

## Existing behavior retained

Tap/Hold modes, 250 ms Hold release grace, saved/unsaved feedback, Server reachable cleanup checks, cleanup diagnostics and raw-text fallback remain. REA 6.8.0 is installed and was used for the first comparison. No further reverse-engineering engine is required for the implemented JavaScript-level workflow comparison; Ghidra/JDK remain optional for a specific future native question.
