# VoiceDictate Privacy Statement

VoiceDictate is local-first software and has no developer-operated backend.

## Data stored on the PC

VoiceDictate stores user settings, text-only dictation history, and local diagnostic logs under the current Windows user's application-data directory. Logs exclude dictated content by default. History can be deleted entry-by-entry or cleared completely from the app.

## Audio handling

Audio is captured only after the configured shortcut is activated. A temporary mono WAV file is created for the isolated transcription worker and deleted after transcription, cancellation, or failure. Audio is not added to history.

## Network behavior

Core dictation makes no network requests. Network access occurs only when the user explicitly:

1. Downloads an optional speech model; or
2. Enables AI cleanup and configures an Ollama or OpenAI-compatible endpoint.

When cleanup is enabled, VoiceDictate sends transcript text and the configured cleanup instructions. It never sends microphone audio to a text model.

## Telemetry and accounts

VoiceDictate contains no telemetry, advertising, analytics, accounts, usage counters, or subscription checks.
