# VoiceDictate and installed Wispr Flow — October 25, 2026

## Scope and result

Historical baseline: this comparison examined VoiceDictate 0.2.1 before the authorized 0.2.2 implementation. Current implementation status is in FEATURE-LIST.md and RELEASE-NOTES.md.

Reviewed the user's complaints, historical backlog, current VoiceDictate source, and installed Wispr Flow 1.6.1264 packaged resources. This is a first static comparison, not a timed dictation benchmark or a complete reconstruction. No VoiceDictate application code or competitor installation was changed. No microphone recording, sign-in or account action was performed.

**No additional software is required tonight for this comparison.** REA is connected, and Wispr Flow's packaged JavaScript exposes the relevant workflows. Ghidra and a compatible JDK are optional later tools for a specific unanswered native-helper question; they would not reveal the service's server-side implementation or measure work-laptop latency.

## Differences relevant to the requested fixes

| Area | VoiceDictate current source | Installed Wispr Flow packaged code | What this means for our update |
| --- | --- | --- | --- |
| Delay after key release | Collects a complete recording, then begins recognition. Local worker is persistent/preloaded, CPU-based and capped at four threads; cleanup and remote fallback can add waits. | Contains paths that submit audio chunks during recording, with gRPC streaming and HTTP chunk/fallback handling, then finalize the request. | Processing can overlap speech in Wispr's network path. First measure our capture, transcription, cleanup and paste stages. Streaming is a larger design choice, not a proven fix for the reported laptop delay. |
| Recording/status pop-up | Positioned once on the primary monitor. Show/hide failures are not reliably caught; status readiness/replay and monitor-change recovery are missing. | Chooses the cursor's display, updates bounds for monitor changes, rebuilds destroyed status windows, shows without focus, and logs placement errors. | Address visibility and accurate status as a primary reliability fix. Neither code review proves actual on-screen reliability. |
| Two app copies | No single-instance guard found. Startup clears a shared temporary audio directory, potentially disrupting another instance. | Uses Electron's single-instance lock and a handler for second launches. | Add instance protection and safe temporary-file ownership early. Duplicate copies remain a possible contributor, not an established latency diagnosis. |
| Choose shortcut by pressing keys | Shortcut is entered as text. | Explicit shortcut-capture state receives pressed-key/release events, validates and saves the chord. | Matches the user's requested press-to-select field. |
| Ctrl + Windows only | Current shortcut library requires a main key such as Space. | Native-helper key events represent Ctrl and Windows separately; validation allows modifier chords and the inspected Windows blocklist omits this combination. | Ctrl+Windows appears supported by static logic; hands-on confirmation remains open. Our implementation needs a compatible keyboard listener. |
| Diagnose slow dictation | Inference duration exists internally but is discarded; no complete phase timing reaches the UI/history. | Contains phase-latency and audio-chunk timing/counter instrumentation. | Add local, non-content timing diagnostics; there is no need to copy external telemetry. |

VoiceDictate's local mode runs Whisper on the PC and can work offline. The inspected Wispr code contains service-connected streaming paths. This is not a like-for-like local inference speed comparison; the review does not establish Wispr's entire offline behavior, active account configuration, accuracy, or actual speed.

## Requested work, in recommended order

1. Prevent duplicate main instances and protect temporary recordings.
2. Measure and reduce the release-to-text delay on the affected laptop. Record app version, mode, model, cleanup state, worker variant and CPU details alongside non-content stage timings.
3. Make the bottom pop-up consistently visible on the relevant display, with correct listening/processing/success/error states and useful failure diagnostics.
4. Add press-to-select shortcut capture and Ctrl+Windows without another key.
5. Add actual transcription-source reporting, microphone refresh and functioning Start with Windows.
6. Follow with history retention/disable-history, Settings spacing, conditional cleanup details and the active-mode label. Keep visual redesign approval separate from reliability fixes.

Tap/Hold modes, 250 ms release grace, saved/unsaved feedback, Server reachable wording, cleanup diagnostics and raw-text fallback already exist in current source. The full ongoing list is [FEATURE-LIST.md](FEATURE-LIST.md).

## Evidence, confidence and remaining work

- Local VoiceDictate source review (retained under work/voicedictate-source-review.md; not distributed): source locations and status of the historical backlog.
- Local Wispr Flow packaged-code review (retained under work/wispr-static-review.md; not distributed): exact extracted filenames, character offsets, observed code paths, inferences and limitations.
- REA desktop inventory parent: `ev_cd660cfcb6f6ce02a189891ee94a037dc9f646b94ad508d123ccdabe73f21b96`.
- REA package-selection analysis parent: `ev_710dadedd0bbb5297023c2dc0259fdabdf4be2eaea06239f131c04de7ff563ce`.
- REA selected main/overlay/status analysis parent: `ev_3cd93b5050b5972eebca99e9bc6b000ce5e304b440bead5203ae5edd534fe0f8`. The main bundle exceeded the analyzer's 1024 MiB heap, and renderer semantic retention was partial. The concrete main-code findings above come from separate bounded inert-text inspection with recorded offsets, not a claimed complete REA call graph. A larger heap could support a later focused semantic investigation; this is not a missing-software blocker.
- Retained REA records exported to work/wispr-rea-evidence.json locally (not distributed).
- ASAR SHA-256: `4c5318617a7081c4e81190eee12a121dc4b97fa80c462ac5e2f72c74ed31355d`.
- Three unpacked native-addon integrity declarations differ from observed companion bytes. REA continued in record-and-continue mode for static inspection; no mismatch was reported for the selected packed JavaScript. These observations do not establish tampering or native behavior.
- Still unresolved: matched real dictation timings; the work laptop's actual bottleneck; reproduction of the missing pop-up; hands-on Ctrl+Windows behavior in Wispr Flow. Static code establishes implemented paths, not their execution in the user's session.
