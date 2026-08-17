use std::{fs, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{
    blocking::{RequestBuilder, Response},
    header::CONTENT_TYPE,
};
use serde_json::Value;
use uuid::Uuid;

use crate::{models::AppSettings, provider};

fn whisper_cpp_endpoint(base_url: &str) -> String {
    let root = base_url.trim_end_matches('/');
    if root.ends_with("/inference") {
        root.to_string()
    } else {
        format!("{root}/inference")
    }
}

fn openai_transcription_endpoint(base_url: &str) -> String {
    let root = base_url.trim_end_matches('/');
    if root.ends_with("/audio/transcriptions") {
        root.to_string()
    } else if root.ends_with("/v1") {
        format!("{root}/audio/transcriptions")
    } else {
        format!("{root}/v1/audio/transcriptions")
    }
}

fn openai_models_endpoint(base_url: &str) -> String {
    let root = base_url.trim_end_matches('/');
    if root.ends_with("/audio/transcriptions") {
        format!("{}/models", root.trim_end_matches("/audio/transcriptions"))
    } else if root.ends_with("/v1") {
        format!("{root}/models")
    } else {
        format!("{root}/v1/models")
    }
}

fn with_key(request: RequestBuilder, api_key: Option<&str>) -> RequestBuilder {
    if let Some(key) = api_key.filter(|value| !value.trim().is_empty()) {
        request.bearer_auth(key)
    } else {
        request
    }
}

fn multipart_wav(wav_path: &Path, fields: &[(&str, &str)]) -> Result<(String, Vec<u8>)> {
    let boundary = format!("voicedictate-{}", Uuid::new_v4());
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"dictation.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&fs::read(wav_path).context("Could not read the temporary recording")?);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok((format!("multipart/form-data; boundary={boundary}"), body))
}

fn response_json(response: Response, provider_name: &str) -> Result<Value> {
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        let summary: String = body.chars().take(500).collect();
        bail!("{provider_name} returned HTTP {status}: {summary}");
    }
    serde_json::from_str(&body)
        .with_context(|| format!("{provider_name} returned an invalid JSON response"))
}

pub fn transcribe(
    settings: &AppSettings,
    api_key: Option<&str>,
    wav_path: &Path,
) -> Result<String> {
    if settings.transcription_base_url.trim().is_empty() {
        bail!("No transcription server URL is configured");
    }
    provider::validate_url(&settings.transcription_base_url)?;
    let timeout = settings.transcription_timeout_seconds.clamp(5, 600);
    let mut fields = vec![("response_format", "json")];
    match settings.transcription_provider.as_str() {
        "whisper-cpp" => {
            let language = if settings.language.trim().is_empty() {
                "auto"
            } else {
                settings.language.as_str()
            };
            fields.push(("language", language));
        }
        "openai-compatible" => {
            if settings.transcription_model.trim().is_empty() {
                bail!("No remote transcription model is configured");
            }
            if settings.language != "auto" && !settings.language.trim().is_empty() {
                fields.push(("language", settings.language.as_str()));
            }
            fields.push(("model", settings.transcription_model.as_str()));
        }
        _ => bail!("The selected transcription server type is unsupported"),
    }
    let (content_type, body) = multipart_wav(wav_path, &fields)?;

    let (request, name) = match settings.transcription_provider.as_str() {
        "whisper-cpp" => (
            with_key(
                provider::client(timeout)?
                    .post(whisper_cpp_endpoint(&settings.transcription_base_url))
                    .header(CONTENT_TYPE, content_type.clone())
                    .body(body.clone()),
                api_key,
            ),
            "The whisper.cpp server",
        ),
        "openai-compatible" => (
            with_key(
                provider::client(timeout)?
                    .post(openai_transcription_endpoint(
                        &settings.transcription_base_url,
                    ))
                    .header(CONTENT_TYPE, content_type)
                    .body(body),
                api_key,
            ),
            "The transcription provider",
        ),
        _ => bail!("The selected transcription server type is unsupported"),
    };

    let value = response_json(request.send()?, name)?;
    value["text"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{name} returned no transcription text"))
}

pub fn test_connection(settings: &AppSettings, api_key: Option<&str>) -> Result<()> {
    if settings.transcription_base_url.trim().is_empty() {
        bail!("Enter a transcription server URL first");
    }
    provider::validate_url(&settings.transcription_base_url)?;
    let timeout = settings.transcription_timeout_seconds.clamp(5, 30);
    let request = match settings.transcription_provider.as_str() {
        "whisper-cpp" => {
            let endpoint = whisper_cpp_endpoint(&settings.transcription_base_url);
            let root = endpoint.trim_end_matches("/inference");
            with_key(
                provider::client(timeout)?.get(if root.is_empty() {
                    settings.transcription_base_url.as_str()
                } else {
                    root
                }),
                api_key,
            )
        }
        "openai-compatible" => with_key(
            provider::client(timeout)?
                .get(openai_models_endpoint(&settings.transcription_base_url)),
            api_key,
        ),
        _ => bail!("The selected transcription server type is unsupported"),
    };
    let response = request.send()?;
    if !response.status().is_success() {
        bail!(
            "The transcription server returned HTTP {}",
            response.status()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        multipart_wav, openai_models_endpoint, openai_transcription_endpoint, whisper_cpp_endpoint,
    };
    use uuid::Uuid;

    #[test]
    fn builds_whisper_cpp_endpoints() {
        assert_eq!(
            whisper_cpp_endpoint("http://192.168.1.2:8080"),
            "http://192.168.1.2:8080/inference"
        );
        assert_eq!(
            whisper_cpp_endpoint("http://192.168.1.2:8080/inference"),
            "http://192.168.1.2:8080/inference"
        );
    }

    #[test]
    fn builds_openai_compatible_endpoints() {
        assert_eq!(
            openai_transcription_endpoint("https://api.example.com/v1"),
            "https://api.example.com/v1/audio/transcriptions"
        );
        assert_eq!(
            openai_transcription_endpoint("https://api.example.com"),
            "https://api.example.com/v1/audio/transcriptions"
        );
        assert_eq!(
            openai_models_endpoint("https://api.example.com/v1/audio/transcriptions"),
            "https://api.example.com/v1/models"
        );
    }

    #[test]
    fn builds_multipart_audio_upload() {
        let path = std::env::temp_dir().join(format!("voicedictate-test-{}.wav", Uuid::new_v4()));
        fs::write(&path, b"RIFF-test-audio").expect("test audio should be written");
        let (content_type, body) =
            multipart_wav(&path, &[("response_format", "json"), ("language", "auto")])
                .expect("multipart body should be built");
        let _ = fs::remove_file(path);
        let printable = String::from_utf8_lossy(&body);
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        assert!(printable.contains("name=\"response_format\"\r\n\r\njson"));
        assert!(printable.contains("name=\"language\"\r\n\r\nauto"));
        assert!(printable.contains("filename=\"dictation.wav\""));
        assert!(printable.contains("RIFF-test-audio"));
    }
}
