export type AppStatus = "idle" | "listening" | "transcribing" | "cleaning" | "success" | "error";
export type ProviderKind = "none" | "ollama" | "openai-compatible";
export type ActivationBehavior = "toggle" | "hold";
export type TranscriptionMode = "local" | "remote" | "automatic";
export type TranscriptionProviderKind = "whisper-cpp" | "openai-compatible";

export interface StatusPayload {
  status: AppStatus;
  text: string;
  activationBehavior: ActivationBehavior;
  revision: number;
}

export interface ProcessingTimings {
  captureMs: number;
  finishMs: number;
  cleanupMs: number;
  pasteMs: number;
  releaseToTextMs: number;
  inferenceMs: number;
  incrementalChunks: number;
  cpuThreads: number;
}

export interface AppSettings {
  shortcut: string;
  activationBehavior: ActivationBehavior;
  microphoneName: string | null;
  language: string;
  model: "tiny" | "base";
  transcriptionMode: TranscriptionMode;
  transcriptionProvider: TranscriptionProviderKind;
  transcriptionBaseUrl: string;
  transcriptionModel: string;
  transcriptionTimeoutSeconds: number;
  transcriptionApiKeyConfigured: boolean;
  cleanupEnabled: boolean;
  provider: ProviderKind;
  providerBaseUrl: string;
  providerModel: string;
  providerApiKeyConfigured: boolean;
  cleanupPrompt: string;
  launchAtStartup: boolean;
}

export interface HistoryEntry {
  id: number;
  rawText: string;
  finalText: string;
  createdAt: string;
  durationMs: number;
  processingMode: "raw" | "clean" | "clean-fallback";
  transcriptionSource: string;
  processingMs: number;
}

export interface DictationResult {
  rawText: string;
  finalText: string;
  durationMs: number;
  processingMode: HistoryEntry["processingMode"];
  warning?: string;
  transcriptionSource: string;
  timings: ProcessingTimings;
}

export const DEFAULT_CLEANUP_PROMPT = `Polish the dictated text. Remove filler words and accidental repetition, and fix punctuation and grammar. Preserve the exact meaning, language, names, URLs, numbers, and code. Return only the final text with no commentary.`;
