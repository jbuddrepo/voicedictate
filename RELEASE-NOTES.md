# VoiceDictate 0.2.2 review build

- Local transcription begins on completed speech passages during natural pauses, before recording stops. Final words remain in the last pass, cleanup runs once, and the complete result is pasted once. No server or account is required for Local mode.
- Remote and Automatic modes can submit pause-delimited WAV passages while recording using the existing configured endpoints. After an Automatic-mode remote failure, remaining passages use local Whisper. This does not add a provider-specific real-time WebSocket protocol.
- Click the shortcut field and press your preferred keys; release them to capture the combination, then Save. Ctrl + Windows alone is supported. Escape cancels a selection. Windows shortcuts continue to reach Windows, so choose a combination that does not conflict with your other apps.
- A second updated app instance brings the existing window forward. Recording files use isolated instance folders rather than shared startup deletion.
- The listening/processing pop-up is positioned in the work area of the display nearest the cursor, without taking focus. Backend state and renderer readiness/revision recovery replace independent frontend show/hide calls. Success stays visible briefly and errors stay visible longer; Hold instructions now say to release the keys.
- History and the latest result identify the transcription source and processing time. Processing details include finishing, cleanup, paste, CPU threads and passages handled while speaking. The measured processing interval begins when the stop command reaches the backend; it excludes Hold mode's intentional 250 ms release grace.
- Refresh microphones without restarting; disabled cleanup fields collapse; the sidebar displays the saved transcription method.
- Release worker builds explicitly enable native C/C++ optimization while retaining separate baseline AVX and AVX2/FMA builds. Automatic CPU selection uses at most eight logical threads and leaves one logical processor available where possible; explicit positive worker thread overrides remain supported.

Existing settings and history migrate in place. Quit the older app from its tray menu before trying this build: an older version does not participate in the new single-instance guard. Continuous speech, short dictations, slower CPUs, remote timeouts and optional cleanup can still produce a wait after stopping. Hands-on microphone, paste, multiple-monitor/DPI and Ctrl + Windows checks remain necessary on the affected work laptop.

## VoiceDictate 0.2.1 beta

Download **VoiceDictate-Setup-0.2.1.exe** for Windows 10/11 x64, or extract the complete portable ZIP.

- Choose **Tap to start/stop** (the default) or **Hold to talk** under Settings → Dictation → Activation behavior, then save. Holding a shortcut in Tap mode no longer stops recording on release.
- Hold mode records for another 250 milliseconds after key release to help capture final sounds. Repeated press events and quick releases during microphone startup are handled consistently.
- Cleanup server checks show the underlying error and distinguish **Server reachable** from a successful model response. Select a model and use **Test cleanup** to send a short sample sentence without recording audio or saving history.
- Ollama cleanup accepts a root URL, `/v1`, `/api/chat`, or `/api/tags`, with surrounding spaces removed. Cold models have up to two minutes to respond; connection establishment has a ten-second timeout.
- Failed cleanup continues to preserve the raw transcript. Regression coverage includes cleanup errors, empty responses, settings migration, and shortcut event ordering.
- The existing saved/unsaved settings indicator is included.

Existing settings and text history are retained. Close VoiceDictate from its tray menu before upgrading. For the first laptop test, select your microphone, use Local transcription with AI cleanup off, save, and dictate into Notepad.

This is an unsigned beta. Windows SmartScreen may display a warning. Recognition quality depends on the microphone, model, and language. The release grace period does not guarantee perfect final-word recognition. Hands-on testing of microphone capture, text insertion, and the user's actual cleanup server remains necessary on the target PC.
