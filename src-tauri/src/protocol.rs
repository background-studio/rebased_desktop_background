use std::net::IpAddr;

use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::models::{parse_hex_color, Anchor, DisplaySettings, FitMode};
use crate::plugin::SCHEMA_VERSION;

pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_MEDIA_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_REVISION_CHARS: usize = 128;
pub const MAX_URL_CHARS: usize = 2048;

pub const DISPLAY_KEYS: &[&str] = &[
    "fit",
    "anchor",
    "opacity",
    "blur",
    "overlayColor",
    "overlayOpacity",
    "editorEnabled",
    "frameEnabled",
];

#[derive(Debug, Deserialize)]
pub struct PluginRequest {
    pub id: String,
    pub cmd: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Clone, Debug)]
pub struct MediaSpec {
    pub url: String,
    pub mime_type: String,
    pub sha256: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug)]
pub struct ConfigureSpec {
    pub revision: String,
    pub media: MediaSpec,
    pub display: DisplaySettings,
}

pub fn parse_request_line(line: &str) -> Result<PluginRequest, String> {
    if line.len() > MAX_REQUEST_BYTES {
        return Err("请求 JSON 过长。".to_string());
    }
    let request: PluginRequest =
        serde_json::from_str(line).map_err(|error| format!("无效请求：{error}"))?;
    if request.id.is_empty() || request.id.len() > 128 {
        return Err("请求 id 无效。".to_string());
    }
    if request.cmd.is_empty() || request.cmd.len() > 64 {
        return Err("请求 cmd 无效。".to_string());
    }
    Ok(request)
}

pub fn parse_configure(params: &Value) -> Result<ConfigureSpec, String> {
    if params.is_null() || !params.is_object() {
        return Err("configure 缺少 params。".to_string());
    }
    let schema_version = params
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| "configure 缺少 schemaVersion。".to_string())?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(format!(
            "不支持的 configure schemaVersion：{schema_version}。"
        ));
    }
    let revision = params
        .get("revision")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "configure 缺少 revision。".to_string())?;
    if revision.len() > MAX_REVISION_CHARS {
        return Err("configure revision 过长。".to_string());
    }
    let media = params
        .get("media")
        .and_then(Value::as_object)
        .ok_or_else(|| "configure 缺少 media。".to_string())?;
    match media.get("kind").and_then(Value::as_str) {
        Some("image") => {}
        Some("video") => return Err("Rebased 只支持图片背景，不支持视频。".to_string()),
        Some(other) => return Err(format!("不支持的媒体 kind：{other}。")),
        None => return Err("configure 缺少 media.kind。".to_string()),
    }
    let url = media
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "configure 缺少 media.url。".to_string())?;
    validate_configure_url(url)?;
    let mime_type = media
        .get("mimeType")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "configure 缺少 media.mimeType。".to_string())?
        .to_ascii_lowercase();
    validate_image_mime(&mime_type)?;
    let sha256 = media
        .get("sha256")
        .and_then(Value::as_str)
        .map(str::trim)
        .ok_or_else(|| "configure 缺少 media.sha256。".to_string())?;
    let sha256 = normalize_sha256(sha256)?;
    let byte_size = media
        .get("byteSize")
        .and_then(Value::as_u64)
        .ok_or_else(|| "configure 缺少 media.byteSize。".to_string())?;
    if byte_size == 0 || byte_size > MAX_MEDIA_BYTES {
        return Err("媒体大小无效或超过 64 MB 上限。".to_string());
    }
    let display = params
        .get("display")
        .ok_or_else(|| "configure 缺少 display。".to_string())?;
    let display = validate_display(display)?;
    Ok(ConfigureSpec {
        revision: revision.to_string(),
        media: MediaSpec {
            url: url.to_string(),
            mime_type,
            sha256,
            byte_size,
        },
        display: DisplaySettings::from_validated(display),
    })
}

pub fn validate_configure_url(value: &str) -> Result<Url, String> {
    if value.len() > MAX_URL_CHARS {
        return Err("媒体地址过长。".to_string());
    }
    let url = Url::parse(value).map_err(|_| "媒体地址无效。".to_string())?;
    if url.scheme() != "http" {
        return Err("仅允许本机回环 HTTP 地址。".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("不允许携带账号信息。".to_string());
    }
    if url.fragment().is_some() {
        return Err("媒体地址不能包含 fragment。".to_string());
    }
    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| "媒体地址缺少主机。".to_string())?;
    if host != "127.0.0.1" && !host.eq_ignore_ascii_case("localhost") {
        return Err("仅允许本机回环 HTTP 地址。".to_string());
    }
    match url.port() {
        Some(0) => return Err("端口无效。".to_string()),
        Some(_) => {}
        None => {
            if url.port_or_known_default() != Some(80) {
                return Err("端口无效。".to_string());
            }
        }
    }
    Ok(url)
}

pub fn validate_display(value: &Value) -> Result<&serde_json::Map<String, Value>, String> {
    let display = value
        .as_object()
        .ok_or_else(|| "configure.display 必须是对象。".to_string())?;
    for key in display.keys() {
        if !DISPLAY_KEYS.contains(&key.as_str()) {
            return Err(format!("不支持的 display 字段：{key}。"));
        }
    }
    if let Some(fit) = display.get("fit") {
        if !fit
            .as_str()
            .is_some_and(|fit| FitMode::parse(fit).is_some())
        {
            return Err("display.fit 无效。".to_string());
        }
    }
    if let Some(anchor) = display.get("anchor") {
        if !anchor
            .as_str()
            .is_some_and(|anchor| Anchor::parse(anchor).is_some())
        {
            return Err("display.anchor 无效。".to_string());
        }
    }
    if let Some(color) = display.get("overlayColor") {
        if !color
            .as_str()
            .is_some_and(|color| parse_hex_color(color).is_some())
        {
            return Err("display.overlayColor 必须是 #RRGGBB。".to_string());
        }
    }
    for (key, minimum, maximum) in [
        ("opacity", 0.0, 1.0),
        ("blur", 0.0, 40.0),
        ("overlayOpacity", 0.0, 0.9),
    ] {
        let Some(value) = display.get(key) else {
            continue;
        };
        let value = value
            .as_f64()
            .ok_or_else(|| format!("display.{key} 必须是数字。"))?;
        if !value.is_finite() || value < minimum || value > maximum {
            return Err(format!("display.{key} 超出范围 {minimum}..={maximum}。"));
        }
    }
    for key in ["editorEnabled", "frameEnabled"] {
        if display.get(key).is_some_and(|value| !value.is_boolean()) {
            return Err(format!("display.{key} 必须是布尔值。"));
        }
    }
    Ok(display)
}

pub fn ip_is_loopback(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.is_loopback(),
        IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return mapped.is_loopback();
            }
            address.is_loopback()
        }
    }
}

pub fn validate_image_mime(mime_type: &str) -> Result<(), String> {
    match mime_type {
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" => Ok(()),
        "image/avif" => {
            Err("Rebased 插件暂不支持 AVIF，请换成 PNG、JPEG、WebP 或 GIF。".to_string())
        }
        other if other.starts_with("video/") => {
            Err("Rebased 只支持图片背景，不支持视频。".to_string())
        }
        other => Err(format!("不支持的媒体类型：{other}。")),
    }
}

pub fn normalize_sha256(value: &str) -> Result<String, String> {
    let normalized = value.to_ascii_lowercase();
    if normalized.len() != 64
        || !normalized
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("media.sha256 无效。".to_string());
    }
    Ok(normalized)
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn configure_with(media: Value, display: Value) -> Value {
        json!({
            "schemaVersion": 1,
            "revision": "rev-1",
            "media": media,
            "display": display
        })
    }

    fn image_media() -> Value {
        json!({
            "url": "http://127.0.0.1:34567/file.png",
            "kind": "image",
            "mimeType": "image/png",
            "sha256": "A".repeat(64),
            "byteSize": 16
        })
    }

    #[test]
    fn rejects_oversized_json() {
        let line = format!(
            "{{\"id\":\"1\",\"cmd\":\"hello\",\"pad\":\"{}\"}}",
            "x".repeat(MAX_REQUEST_BYTES)
        );
        assert!(parse_request_line(&line).is_err());
    }

    #[test]
    fn parses_hello_without_params() {
        let request = parse_request_line(r#"{"id":"1","cmd":"hello"}"#).unwrap();
        assert_eq!(request.cmd, "hello");
        assert!(request.params.is_null());
    }

    #[test]
    fn rejects_userinfo_non_loopback_https_and_bad_ports() {
        assert!(validate_configure_url("http://127.0.0.1:17890/media").is_ok());
        assert!(validate_configure_url("http://localhost:17890/media").is_ok());
        for url in [
            "https://127.0.0.1:17890/media",
            "http://user@127.0.0.1:17890/media",
            "http://user:pass@127.0.0.1:17890/media",
            "http://192.168.1.2:17890/media",
            "http://8.8.8.8/media",
            "http://example.com/media",
            "http://[::1]:17890/media",
            "http://127.0.0.1:0/media",
            "http://127.0.0.1:17890/media#fragment",
            "file:///C:/secret.png",
        ] {
            assert!(validate_configure_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn accepts_valid_image_configure() {
        let spec = parse_configure(&configure_with(
            image_media(),
            json!({ "opacity": 0.4, "fit": "original", "anchor": "top-left" }),
        ))
        .unwrap();
        assert_eq!(spec.revision, "rev-1");
        assert_eq!(spec.media.sha256, "a".repeat(64));
        assert_eq!(spec.display.opacity, 0.4);
        assert_eq!(spec.display.fit, FitMode::Original);
        assert_eq!(spec.display.anchor, Anchor::TopLeft);
    }

    #[test]
    fn rejects_video_and_avif_with_explanations() {
        let mut video = image_media();
        video["kind"] = json!("video");
        video["mimeType"] = json!("video/mp4");
        let error = parse_configure(&configure_with(video, json!({}))).unwrap_err();
        assert!(error.contains("不支持视频"), "{error}");

        let mut avif = image_media();
        avif["mimeType"] = json!("image/avif");
        let error = parse_configure(&configure_with(avif, json!({}))).unwrap_err();
        assert!(error.contains("AVIF"), "{error}");
    }

    #[test]
    fn rejects_invalid_display() {
        for display in [
            json!({ "fit": "contain" }),
            json!({ "anchor": "middle" }),
            json!({ "opacity": 1.5 }),
            json!({ "blur": -1 }),
            json!({ "overlayOpacity": 0.95 }),
            json!({ "overlayColor": "red" }),
            json!({ "editorEnabled": "yes" }),
            json!({ "sidebarOpacity": 0.2 }),
        ] {
            assert!(
                parse_configure(&configure_with(image_media(), display.clone())).is_err(),
                "{display}"
            );
        }
    }

    #[test]
    fn rejects_invalid_media_fields() {
        let mut bad_hash = image_media();
        bad_hash["sha256"] = json!("zz");
        assert!(parse_configure(&configure_with(bad_hash, json!({}))).is_err());
        let mut too_big = image_media();
        too_big["byteSize"] = json!(MAX_MEDIA_BYTES + 1);
        assert!(parse_configure(&configure_with(too_big, json!({}))).is_err());
        assert!(parse_configure(&json!({})).is_err());
    }
}
