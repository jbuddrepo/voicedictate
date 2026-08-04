export type AppStatus = "idle" | "listening" | "transcribing" | "cleaning" | "success" | "error";
export type ProviderKind = "none" | "ollama" | "openai-compatible";

export interface AppSettings {
  shortcut: string;
  microphoneName: string | null;
  language: string;
  model: "tiny" | "base";
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
}

export interface DictationResult {
  rawText: string;
  finalText: string;
  durationMs: number;
  processingMode: HistoryEntry["processingMode"];
  warning?: string;
}

export const DEFAULT_CLEANUP_PROMPT = `Polish the dictated text. Remove filler words and accidental repetition, and fix punctuation and grammar. Preserve the exact meaning, language, names, URLs, numbers, and code. Return only the final text with no commentary.`;
