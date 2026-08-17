# VoiceDictate Privacy Statement

VoiceDictate is local-first software and has no developer-operated backend.

## Data stored on the PC

VoiceDictate stores user settings, text-only dictation history, and local diagnostic logs under the current Windows user's application-data directory. Logs exclude dictated content by default. History can be deleted entry-by-entry or cleared completely from the app.

## Audio handling

Audio is captured only after the configured shortcut is activated. A temporary mono WAV file is created and deleted after transcription, cancellation, or failure. Audio is not added to history. In Local mode it is read only by the isolated transcription worker. In Remote or Automatic mode it is sent to the transcription endpoint explicitly configured by the user. Automatic mode may read the same temporary file locally if the remote request fails.

## Network behavior

Local dictation makes no network requests. Network access occurs only when the user explicitly:

1. Downloads an optional speech model; or
2. Selects Remote or Automatic transcription and configures a whisper.cpp or OpenAI-compatible audio endpoint; or
3. Enables AI cleanup and configures an Ollama or OpenAI-compatible text endpoint.

Remote transcription sends microphone audio to the configured transcription endpoint. When cleanup is enabled, VoiceDictate sends transcript text and the configured cleanup instructions to the separately configured cleanup endpoint. It never sends microphone audio to an Ollama or other text-cleanup model. API keys are stored using Windows Credential Manager and are not written to the settings file.

## Telemetry and accounts

VoiceDictate contains no telemetry, advertising, analytics, accounts, usage counters, or subscription checks.
