use serde::{Deserialize, Serialize};

pub const DEFAULT_CLEANUP_PROMPT: &str = "Polish the dictated text. Remove filler words and accidental repetition, and fix punctuation and grammar. Preserve the exact meaning, language, names, URLs, numbers, and code. Return only the final text with no commentary.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub shortcut: String,
    pub microphone_name: Option<String>,
    pub language: String,
    pub model: String,
    pub cleanup_enabled: bool,
    pub provider: String,
    pub provider_base_url: String,
    pub provider_model: String,
    #[serde(default, skip_deserializing)]
    pub provider_api_key_configured: bool,
    pub cleanup_prompt: String,
    pub launch_at_startup: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: "Ctrl+Alt+Space".into(),
            microphone_name: None,
            language: "auto".into(),
            model: "tiny".into(),
            cleanup_enabled: false,
            provider: "none".into(),
            provider_base_url: "http://127.0.0.1:11434".into(),
            provider_model: String::new(),
            provider_api_key_configured: false,
            cleanup_prompt: DEFAULT_CLEANUP_PROMPT.into(),
            launch_at_startup: false,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationResult {
    pub raw_text: String,
    pub final_text: String,
    pub duration_ms: u64,
    pub processing_mode: String,
    pub warning: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerResponse {
    pub text: String,
    #[serde(rename = "inferenceMs")]
    pub _inference_ms: u64,
    #[serde(rename = "language")]
    pub _language: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}
