use std::{net::IpAddr, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use url::Url;

use crate::models::AppSettings;

pub(crate) fn validate_url(raw: &str) -> Result<Url> {
    let parsed = Url::parse(raw.trim()).context("The provider URL is invalid")?;
    if parsed.scheme() == "https" {
        return Ok(parsed);
    }
    if parsed.scheme() != "http" {
        bail!("Provider URLs must use HTTPS, or HTTP on a private network");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("The provider URL has no host"))?;
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(parsed);
    }
    let ip: IpAddr = host.parse().map_err(|_| {
        anyhow!("HTTP is allowed only for localhost or a private/Tailscale IP address")
    })?;
    let allowed = match ip {
        IpAddr::V4(value) => {
            value.is_loopback() || value.is_private() || value.is_link_local() || {
                let octets = value.octets();
                octets[0] == 100 && (64..=127).contains(&octets[1])
            }
        }
        IpAddr::V6(value) => {
            value.is_loopback() || value.is_unique_local() || value.is_unicast_link_local()
        }
    };
    if !allowed {
        bail!("Public provider endpoints must use HTTPS");
    }
    Ok(parsed)
}

pub(crate) fn client(timeout_seconds: u64) -> Result<Client> {
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_seconds))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

fn ollama_root(raw: &str) -> String {
    raw.trim()
        .trim_end_matches('/')
        .trim_end_matches("/api/chat")
        .trim_end_matches("/api/tags")
        .trim_end_matches("/v1")
        .to_string()
}

pub fn discover_models(settings: &AppSettings, api_key: Option<&str>) -> Result<Vec<String>> {
    validate_url(&settings.provider_base_url)?;
    match settings.provider.as_str() {
        "ollama" => {
            let response: Value = client(10)?
                .get(format!(
                    "{}/api/tags",
                    ollama_root(&settings.provider_base_url)
                ))
                .send()?
                .error_for_status()?
                .json()?;
            Ok(response["models"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item["name"].as_str().map(str::to_string))
                .collect())
        }
        "openai-compatible" => {
            let root = settings.provider_base_url.trim().trim_end_matches('/');
            let endpoint = if root.ends_with("/v1") {
                format!("{root}/models")
            } else {
                format!("{root}/v1/models")
            };
            let mut request = client(10)?.get(endpoint);
            if let Some(key) = api_key.filter(|key| !key.is_empty()) {
                request = request.bearer_auth(key);
            }
            let response: Value = request.send()?.error_for_status()?.json()?;
            Ok(response["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item["id"].as_str().map(str::to_string))
                .collect())
        }
        _ => bail!("Select Ollama or an OpenAI-compatible cleanup provider first"),
    }
}

pub fn clean_text(settings: &AppSettings, api_key: Option<&str>, text: &str) -> Result<String> {
    if !settings.cleanup_enabled || settings.provider == "none" {
        return Ok(text.to_string());
    }
    validate_url(&settings.provider_base_url)?;
    if settings.provider_model.trim().is_empty() {
        bail!("No cleanup model is selected");
    }
    let messages = json!([
        {"role": "system", "content": settings.cleanup_prompt},
        {"role": "user", "content": text}
    ]);
    let response: Value = match settings.provider.as_str() {
        // Allow a cold Ollama model to load; unreachable servers still fail quickly.
        "ollama" => client(120)?
            .post(format!(
                "{}/api/chat",
                ollama_root(&settings.provider_base_url)
            ))
            .json(&json!({
                "model": settings.provider_model, "messages": messages, "stream": false
            }))
            .send()?
            .error_for_status()?
            .json()?,
        "openai-compatible" => {
            let root = settings.provider_base_url.trim().trim_end_matches('/');
            let endpoint = if root.ends_with("/v1") {
                format!("{root}/chat/completions")
            } else {
                format!("{root}/v1/chat/completions")
            };
            let mut request = client(45)?.post(endpoint).json(&json!({"model": settings.provider_model, "messages": messages, "temperature": 0.1}));
            if let Some(key) = api_key.filter(|key| !key.is_empty()) {
                request = request.bearer_auth(key);
            }
            request.send()?.error_for_status()?.json()?
        }
        _ => return Ok(text.to_string()),
    };
    let value = if settings.provider == "ollama" {
        response["message"]["content"].as_str()
    } else {
        response["choices"][0]["message"]["content"].as_str()
    };
    let cleaned = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("The cleanup provider returned no text"))?;
    Ok(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        thread,
    };

    fn mock_server(body: &'static str, status: u16) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(!line.is_empty());
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut data = vec![0; length];
            reader.read_exact(&mut data).unwrap();
            request.push_str(&String::from_utf8(data).unwrap());
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            request
        });
        (url, task)
    }

    #[test]
    fn ollama_cleanup_uses_normalized_url_and_non_streaming_chat() {
        let (url, server) = mock_server(r#"{"message":{"content":"Synthetic result."}}"#, 200);
        let settings = AppSettings {
            cleanup_enabled: true,
            provider: "ollama".into(),
            provider_base_url: format!("  {url}/api/chat/  "),
            provider_model: "test-model".into(),
            ..AppSettings::default()
        };
        assert_eq!(
            clean_text(&settings, None, "Synthetic input.").unwrap(),
            "Synthetic result."
        );
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /api/chat HTTP/1.1"));
        assert!(request.contains("\"stream\":false"));
        assert!(request.contains("test-model"));
    }

    #[test]
    fn ollama_model_discovery_accepts_v1_root() {
        let (url, server) = mock_server(r#"{"models":[{"name":"test-model"}]}"#, 200);
        let settings = AppSettings {
            provider: "ollama".into(),
            provider_base_url: format!("{url}/v1/"),
            ..AppSettings::default()
        };
        assert_eq!(
            discover_models(&settings, None).unwrap(),
            vec!["test-model"]
        );
        assert!(server.join().unwrap().starts_with("GET /api/tags HTTP/1.1"));
    }

    #[test]
    fn failed_cleanup_http_status_is_an_error() {
        let (url, server) = mock_server("{}", 503);
        let settings = AppSettings {
            cleanup_enabled: true,
            provider: "ollama".into(),
            provider_base_url: url,
            provider_model: "test-model".into(),
            ..AppSettings::default()
        };
        assert!(clean_text(&settings, None, "Synthetic input.").is_err());
        server.join().unwrap();
    }

    #[test]
    fn empty_cleanup_response_is_an_error() {
        let (url, server) = mock_server(r#"{"message":{"content":"  "}}"#, 200);
        let settings = AppSettings {
            cleanup_enabled: true,
            provider: "ollama".into(),
            provider_base_url: url,
            provider_model: "test-model".into(),
            ..AppSettings::default()
        };
        assert!(clean_text(&settings, None, "Synthetic input.").is_err());
        server.join().unwrap();
    }
    #[test]
    fn permits_private_and_tailscale_http() {
        assert!(validate_url("http://127.0.0.1:11434").is_ok());
        assert!(validate_url("http://192.168.1.2:11434").is_ok());
        assert!(validate_url("http://100.64.0.10:11434").is_ok());
    }
    #[test]
    fn rejects_public_http() {
        assert!(validate_url("http://8.8.8.8:11434").is_err());
    }
}
