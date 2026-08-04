use std::{net::IpAddr, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use url::Url;

use crate::models::AppSettings;

fn validate_url(raw: &str) -> Result<Url> {
    let parsed = Url::parse(raw).context("The provider URL is invalid")?;
    if parsed.scheme() == "https" { return Ok(parsed); }
    if parsed.scheme() != "http" { bail!("Provider URLs must use HTTPS, or HTTP on a private network"); }
    let host = parsed.host_str().ok_or_else(|| anyhow!("The provider URL has no host"))?;
    if host.eq_ignore_ascii_case("localhost") { return Ok(parsed); }
    let ip: IpAddr = host.parse().map_err(|_| anyhow!("HTTP is allowed only for localhost or a private/Tailscale IP address"))?;
    let allowed = match ip {
        IpAddr::V4(value) => value.is_loopback() || value.is_private() || value.is_link_local() || {
            let octets = value.octets(); octets[0] == 100 && (64..=127).contains(&octets[1])
        },
        IpAddr::V6(value) => value.is_loopback() || value.is_unique_local() || value.is_unicast_link_local(),
    };
    if !allowed { bail!("Public provider endpoints must use HTTPS"); }
    Ok(parsed)
}

fn client(timeout_seconds: u64) -> Result<Client> {
    Ok(Client::builder().timeout(Duration::from_secs(timeout_seconds)).build()?)
}

fn ollama_root(raw: &str) -> String {
    raw.trim_end_matches('/').trim_end_matches("/v1").to_string()
}

pub fn discover_models(settings: &AppSettings, api_key: Option<&str>) -> Result<Vec<String>> {
    validate_url(&settings.provider_base_url)?;
    match settings.provider.as_str() {
        "ollama" => {
            let response: Value = client(10)?.get(format!("{}/api/tags", ollama_root(&settings.provider_base_url))).send()?.error_for_status()?.json()?;
            Ok(response["models"].as_array().into_iter().flatten().filter_map(|item| item["name"].as_str().map(str::to_string)).collect())
        }
        "openai-compatible" => {
            let root = settings.provider_base_url.trim_end_matches('/');
            let endpoint = if root.ends_with("/v1") { format!("{root}/models") } else { format!("{root}/v1/models") };
            let mut request = client(10)?.get(endpoint);
            if let Some(key) = api_key.filter(|key| !key.is_empty()) { request = request.bearer_auth(key); }
            let response: Value = request.send()?.error_for_status()?.json()?;
            Ok(response["data"].as_array().into_iter().flatten().filter_map(|item| item["id"].as_str().map(str::to_string)).collect())
        }
        _ => Ok(Vec::new()),
    }
}

pub fn clean_text(settings: &AppSettings, api_key: Option<&str>, text: &str) -> Result<String> {
    if !settings.cleanup_enabled || settings.provider == "none" { return Ok(text.to_string()); }
    validate_url(&settings.provider_base_url)?;
    if settings.provider_model.trim().is_empty() { bail!("No cleanup model is selected"); }
    let messages = json!([
        {"role": "system", "content": settings.cleanup_prompt},
        {"role": "user", "content": text}
    ]);
    let response: Value = match settings.provider.as_str() {
        "ollama" => client(45)?.post(format!("{}/api/chat", ollama_root(&settings.provider_base_url))).json(&json!({
            "model": settings.provider_model, "messages": messages, "stream": false
        })).send()?.error_for_status()?.json()?,
        "openai-compatible" => {
            let root = settings.provider_base_url.trim_end_matches('/');
            let endpoint = if root.ends_with("/v1") { format!("{root}/chat/completions") } else { format!("{root}/v1/chat/completions") };
            let mut request = client(45)?.post(endpoint).json(&json!({"model": settings.provider_model, "messages": messages, "temperature": 0.1}));
            if let Some(key) = api_key.filter(|key| !key.is_empty()) { request = request.bearer_auth(key); }
            request.send()?.error_for_status()?.json()?
        }
        _ => return Ok(text.to_string()),
    };
    let value = if settings.provider == "ollama" { response["message"]["content"].as_str() } else { response["choices"][0]["message"]["content"].as_str() };
    let cleaned = value.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| anyhow!("The cleanup provider returned no text"))?;
    Ok(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::validate_url;
    #[test]
    fn permits_private_and_tailscale_http() {
        assert!(validate_url("http://127.0.0.1:11434").is_ok());
        assert!(validate_url("http://192.168.1.2:11434").is_ok());
        assert!(validate_url("http://100.64.0.10:11434").is_ok());
    }
    #[test]
    fn rejects_public_http() { assert!(validate_url("http://8.8.8.8:11434").is_err()); }
}
