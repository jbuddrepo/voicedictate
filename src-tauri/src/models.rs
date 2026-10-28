use serde::{Deserialize, Serialize};

pub const DEFAULT_CLEANUP_PROMPT: &str = "Polish the dictated text. Remove filler words and accidental repetition, and fix punctuation and grammar. Preserve the exact meaning, language, names, URLs, numbers, and code. Return only the final text with no commentary.";

fn default_transcription_mode() -> String {
    "local".into()
}

fn default_activation_behavior() -> String {
    "toggle".into()
}

fn default_transcription_provider() -> String {
    "whisper-cpp".into()
}

fn default_transcription_timeout_seconds() -> u64 {
    90
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub shortcut: String,
    #[serde(default = "default_activation_behavior")]
    pub activation_behavior: String,
    pub microphone_name: Option<String>,
    pub language: String,
    pub model: String,
    #[serde(default = "default_transcription_mode")]
    pub transcription_mode: String,
    #[serde(default = "default_transcription_provider")]
    pub transcription_provider: String,
    #[serde(default)]
    pub transcription_base_url: String,
    #[serde(default)]
    pub transcription_model: String,
    #[serde(default = "default_transcription_timeout_seconds")]
    pub transcription_timeout_seconds: u64,
    #[serde(default, skip_deserializing)]
    pub transcription_api_key_configured: bool,
    pub cleanup_enabled: bool,
    pub provider: String,
    pub provider_base_url: String,
    pub provider_model: String,
    #[serde(default, skip_deserializing)]
    pub provider_api_key_configured: bool,
    pub cleanup_prompt: String,
    pub launch_at_startup: bool,
    #[serde(default)]
    pub personal_vocabulary: Vec<crate::vocabulary::VocabularyEntry>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: "Ctrl+Alt+Space".into(),
            activation_behavior: default_activation_behavior(),
            microphone_name: None,
            language: "auto".into(),
            model: "tiny".into(),
            transcription_mode: default_transcription_mode(),
            transcription_provider: default_transcription_provider(),
            transcription_base_url: String::new(),
            transcription_model: String::new(),
            transcription_timeout_seconds: default_transcription_timeout_seconds(),
            transcription_api_key_configured: false,
            cleanup_enabled: false,
            provider: "none".into(),
            provider_base_url: "http://127.0.0.1:11434".into(),
            provider_model: String::new(),
            provider_api_key_configured: false,
            cleanup_prompt: DEFAULT_CLEANUP_PROMPT.into(),
            launch_at_startup: false,
            personal_vocabulary: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub raw_text: String,
    pub final_text: String,
    pub created_at: String,
    pub duration_ms: u64,
    pub processing_mode: String,
    pub transcription_source: String,
    pub processing_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingTimings {
    pub capture_ms: u64,
    pub finish_ms: u64,
    pub cleanup_ms: u64,
    pub paste_ms: u64,
    pub release_to_text_ms: u64,
    pub inference_ms: u64,
    pub incremental_chunks: usize,
    pub cpu_threads: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub status: String,
    pub text: String,
    pub activation_behavior: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationResult {
    pub raw_text: String,
    pub final_text: String,
    pub duration_ms: u64,
    pub processing_mode: String,
    pub warning: Option<String>,
    #[serde(default)]
    pub transcription_source: String,
    #[serde(default)]
    pub timings: ProcessingTimings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerResponse {
    pub text: String,
    pub inference_ms: u64,
    #[serde(default)]
    pub threads: usize,
    #[serde(rename = "language")]
    pub _language: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn older_settings_default_to_local_transcription() {
        let old = r#"{
          "shortcut":"Ctrl+Alt+Space",
          "microphoneName":null,
          "language":"auto",
          "model":"tiny",
          "cleanupEnabled":false,
          "provider":"none",
          "providerBaseUrl":"http://127.0.0.1:11434",
          "providerModel":"",
          "cleanupPrompt":"test",
          "launchAtStartup":false
        }"#;
        let settings: AppSettings = serde_json::from_str(old).expect("old settings should migrate");
        assert_eq!(settings.transcription_mode, "local");
        assert_eq!(settings.activation_behavior, "toggle");
        assert_eq!(settings.transcription_provider, "whisper-cpp");
        assert_eq!(settings.transcription_timeout_seconds, 90);
        assert!(settings.personal_vocabulary.is_empty());
    }
}
