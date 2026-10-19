# VoiceDictate 0.2.1 beta

Download **VoiceDictate-Setup-0.2.1.exe** for Windows 10/11 x64, or extract the complete portable ZIP.

- Choose **Tap to start/stop** (the default) or **Hold to talk** under Settings → Dictation → Activation behavior, then save. Holding a shortcut in Tap mode no longer stops recording on release.
- Hold mode records for another 250 milliseconds after key release to help capture final sounds. Repeated press events and quick releases during microphone startup are handled consistently.
- Cleanup server checks show the underlying error and distinguish **Server reachable** from a successful model response. Select a model and use **Test cleanup** to send a short sample sentence without recording audio or saving history.
- Ollama cleanup accepts a root URL, `/v1`, `/api/chat`, or `/api/tags`, with surrounding spaces removed. Cold models have up to two minutes to respond; connection establishment has a ten-second timeout.
- Failed cleanup continues to preserve the raw transcript. Regression coverage includes cleanup errors, empty responses, settings migration, and shortcut event ordering.
- The existing saved/unsaved settings indicator is included.

Existing settings and text history are retained. Close VoiceDictate from its tray menu before upgrading. For the first laptop test, select your microphone, use Local transcription with AI cleanup off, save, and dictate into Notepad.

This is an unsigned beta. Windows SmartScreen may display a warning. Recognition quality depends on the microphone, model, and language. The release grace period does not guarantee perfect final-word recognition. Hands-on testing of microphone capture, text insertion, and the user's actual cleanup server remains necessary on the target PC.
