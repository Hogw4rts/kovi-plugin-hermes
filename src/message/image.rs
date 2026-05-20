use base64::Engine;
use regex::Regex;
use std::sync::LazyLock;

use crate::config::ImageMode;

static IMAGE_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"https?://[^\s<>"']++\.(?:jpg|jpeg|png|gif|webp|bmp|svg)(?:\?[^\s<>"']*)?"##)
        .expect("invalid image url regex")
});
static QQ_AVATAR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https?://(?:q\d*\.qlogo\.cn|img\.qq\.com)/[^\s<>"']+"#)
        .expect("invalid qq avatar url regex")
});
static QQ_CDN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https?://multimedia\.nt\.qq\.com\.cn/[^\s<>"']+"#)
        .expect("invalid qq cdn url regex")
});

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

fn is_ssrf_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or("");

    if rest.is_empty() {
        return true;
    }

    let host_port = rest.split('/').next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        if let Some(end) = host_port.find(']') {
            &host_port[..=end]
        } else {
            host_port
        }
    } else {
        host_port.split(':').next().unwrap_or("")
    };

    if host.is_empty() {
        return true;
    }

    if host == "localhost" || host == "127.0.0.1" || host == "0.0.0.0" || host == "[::1]" || host == "[::ffff:127.0.0.1]" {
        return true;
    }

    if host.starts_with("10.")
        || host.starts_with("192.168.")
        || (host.starts_with("172.")
            && host.split('.').nth(1).and_then(|o| o.parse::<u8>().ok()).is_some_and(|o| (16..=31).contains(&o)))
    {
        return true;
    }

    if host.starts_with("169.254.") || host.starts_with("100.64.") {
        return true;
    }

    if host.ends_with(".internal") || host.ends_with(".local") || host == "metadata.google.internal" {
        return true;
    }

    if let Some(ip_str) = host.strip_prefix("[::ffff:")
        && let Some(ip_str) = ip_str.strip_suffix(']')
        && is_private_ipv4(ip_str)
    {
        return true;
    }

    if let Ok(num) = host.parse::<u32>() {
        let a = (num >> 24) & 0xFF;
        let b = (num >> 16) & 0xFF;
        if a == 127 || a == 10 || a == 0 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168) {
            return true;
        }
    }

    if host.starts_with("0x") || host.starts_with("0X") {
        return true;
    }

    if host.contains('-') && !host.contains('.') {
        return true;
    }

    false
}

fn is_private_ipv4(ip: &str) -> bool {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let a = parts[0].parse::<u8>().unwrap_or(255);
    let b = parts[1].parse::<u8>().unwrap_or(255);
    a == 127 || a == 10 || a == 0
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 169 && b == 254)
        || (a == 100 && (64..=127).contains(&b))
}

fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        url.to_string()
    } else {
        let boundary = url
            .char_indices()
            .take_while(|(i, _)| *i < max_len)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if boundary == 0 {
            let first_char_end = url.char_indices().nth(1).map(|(i, _)| i).unwrap_or(url.len());
            format!("{}...", &url[..first_char_end])
        } else {
            format!("{}...", &url[..boundary])
        }
    }
}

async fn process_image_url(
    http: &reqwest::Client,
    url: String,
    mode: ImageMode,
) -> Option<String> {
    if is_ssrf_url(&url) {
        kovi::log::warn!("hermes: blocked SSRF image URL: {}", truncate_url(&url, 80));
        return None;
    }

    let needs_base64 = url.contains("multimedia.nt.qq.com.cn");
    match mode {
        ImageMode::Url if !needs_base64 => {
            kovi::log::info!("hermes: image URL passthrough ({})", truncate_url(&url, 80));
            Some(url)
        }
        _ => match download_as_base64(http, &url).await {
            Ok(data_uri) => {
                kovi::log::info!("hermes: image downloaded as base64 ({} bytes from {})", data_uri.len(), truncate_url(&url, 80));
                Some(data_uri)
            }
            Err(e) => {
                kovi::log::warn!("hermes: failed to download image {}: {e}", truncate_url(&url, 80));
                None
            }
        },
    }
}

pub(crate) async fn extract_image_urls(
    bot: &kovi::RuntimeBot,
    message: &kovi::Message,
    http: &reqwest::Client,
    mode: ImageMode,
) -> Vec<String> {
    let segments = message.get("image");
    let mut results = Vec::with_capacity(segments.len().max(4));

    for seg in &segments {
        let raw_url = seg
            .data
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && (s.starts_with("http://") || s.starts_with("https://")));

        let raw_url = match raw_url {
            Some(u) => Some(u.to_string()),
            None => {
                let file_val = seg.data.get("file").and_then(|v| v.as_str());
                match file_val {
                    Some(f) if f.starts_with("http://") || f.starts_with("https://") => {
                        Some(f.to_string())
                    }
                    Some(f) if !f.is_empty() => {
                        match bot.get_image(f).await {
                            Ok(resp) => resp
                                .data
                                .get("url")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                            Err(_) => {
                                kovi::log::warn!("hermes: get_image API failed for file={f}");
                                None
                            }
                        }
                    }
                    _ => None,
                }
            }
        };

        let Some(url) = raw_url else {
            kovi::log::warn!(
                "hermes: image segment has no resolvable URL: {:?}",
                seg.data
            );
            continue;
        };

        if let Some(result) = process_image_url(http, url, mode).await {
            results.push(result);
        }
    }

    let text_segments = message.get("text");
    let text: &str = text_segments
        .first()
        .and_then(|seg| seg.data.get("text").and_then(|v| v.as_str()))
        .unwrap_or("");

    let text_image_urls = extract_text_image_urls(text);
    for url in text_image_urls {
        if results.iter().any(|u| u == &url) {
            continue;
        }
        if let Some(result) = process_image_url(http, url, mode).await {
            results.push(result);
        }
    }

    results
}

pub(crate) async fn extract_reply_image_urls(
    bot: &kovi::RuntimeBot,
    message: &kovi::Message,
    http: &reqwest::Client,
    mode: ImageMode,
) -> Vec<String> {
    let reply_segments = message.get("reply");
    if reply_segments.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    for seg in &reply_segments {
        let Some(id_str) = seg.data.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(msg_id) = id_str.parse::<i32>() else {
            continue;
        };

        let Ok(resp) = bot.get_msg(msg_id).await else {
            kovi::log::warn!("hermes: get_msg failed for reply id={msg_id}");
            continue;
        };

        let Some(msg_array) = resp.data.get("message").and_then(|v| v.as_array()) else {
            continue;
        };

        for item in msg_array {
            let Some(type_) = item.get("type").and_then(|v| v.as_str()) else {
                continue;
            };
            if type_ != "image" {
                continue;
            }

            let Some(data) = item.get("data") else {
                continue;
            };

            let raw_url = data
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && (s.starts_with("http://") || s.starts_with("https://")));

            let url = match raw_url {
                Some(u) => Some(u.to_string()),
                None => {
                    let file_val = data.get("file").and_then(|v| v.as_str());
                    match file_val {
                        Some(f) if f.starts_with("http://") || f.starts_with("https://") => {
                            Some(f.to_string())
                        }
                        Some(f) if !f.is_empty() => {
                            match bot.get_image(f).await {
                                Ok(img_resp) => img_resp
                                    .data
                                    .get("url")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                    .map(|s| s.to_string()),
                                Err(_) => None,
                            }
                        }
                        _ => None,
                    }
                }
            };

            let Some(url) = url else {
                continue;
            };

            if let Some(result) = process_image_url(http, url, mode).await {
                results.push(result);
            }
        }
    }

    results
}

fn extract_text_image_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for re in [&*IMAGE_URL_RE, &*QQ_AVATAR_RE, &*QQ_CDN_RE] {
        for cap in re.captures_iter(text) {
            let url = cap[0].to_string();
            if seen.insert(url.clone()) {
                urls.push(url);
            }
        }
    }

    urls
}

async fn download_as_base64(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }

    if let Some(content_len) = resp.headers().get("content-length")
        && let Ok(s) = content_len.to_str()
        && let Ok(len) = s.parse::<usize>()
        && len > MAX_IMAGE_BYTES
    {
        return Err(format!("image too large (Content-Length: {len} bytes)"));
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();

    let mime = if content_type.starts_with("image/") {
        content_type
    } else {
        "image/png".to_string()
    };

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("read body failed: {e}"))?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!("image too large: {} bytes", bytes.len()));
    }

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("{mime};base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssrf_localhost() {
        assert!(is_ssrf_url("http://localhost/admin"));
        assert!(is_ssrf_url("http://127.0.0.1:8080/api"));
        assert!(is_ssrf_url("http://0.0.0.0/"));
        assert!(is_ssrf_url("http://[::1]/"));
        assert!(is_ssrf_url("http://[::ffff:127.0.0.1]/"));
    }

    #[test]
    fn test_ssrf_private_ip() {
        assert!(is_ssrf_url("http://10.0.0.1/secret"));
        assert!(is_ssrf_url("http://192.168.1.1/router"));
        assert!(is_ssrf_url("http://172.16.0.1/"));
        assert!(is_ssrf_url("http://172.31.255.255/"));
    }

    #[test]
    fn test_ssrf_metadata() {
        assert!(is_ssrf_url("http://169.254.169.254/latest/meta-data/"));
        assert!(is_ssrf_url("http://metadata.google.internal/"));
        assert!(is_ssrf_url("http://100.64.0.1/"));
    }

    #[test]
    fn test_ssrf_decimal_ip() {
        assert!(is_ssrf_url("http://2130706433/"));
        assert!(is_ssrf_url("http://167772161/"));
    }

    #[test]
    fn test_ssrf_hex_prefix() {
        assert!(is_ssrf_url("http://0x7f000001/"));
        assert!(is_ssrf_url("http://0X0a000001/"));
    }

    #[test]
    fn test_ssrf_ipv6_mapped() {
        assert!(is_ssrf_url("http://[::ffff:10.0.0.1]/"));
        assert!(is_ssrf_url("http://[::ffff:192.168.1.1]/"));
    }

    #[test]
    fn test_ssrf_allowed() {
        assert!(!is_ssrf_url("https://api.openai.com/v1/chat"));
        assert!(!is_ssrf_url("https://example.com/image.png"));
        assert!(!is_ssrf_url("https://multimedia.nt.qq.com.cn/"));
    }

    #[test]
    fn test_truncate_url_short() {
        assert_eq!(truncate_url("https://x.com/img.png", 80), "https://x.com/img.png");
    }

    #[test]
    fn test_truncate_url_long() {
        let long_url = "https://example.com/very/long/path/that/exceeds/max";
        let truncated = truncate_url(long_url, 20);
        assert!(truncated.ends_with("..."));
        assert!(truncated.len() <= 23);
    }

    #[test]
    fn test_truncate_url_multibyte() {
        let url = "https://example.com/日本語/画像.png";
        let truncated = truncate_url(url, 25);
        assert!(truncated.ends_with("..."));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }
}