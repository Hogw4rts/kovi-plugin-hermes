use regex::Regex;
use std::sync::LazyLock;
use base64::Engine;

use crate::config::ImageMode;

static THINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)\U0001f9e0[\s\S]*?\U0001f9e0").expect("invalid think regex"));
static ASSISTANT_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*assistant\s*:\s*").expect("invalid assistant regex"));
static FENCED_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)```[a-z0-9_-]*\n?").expect("invalid fenced code regex"));
static INLINE_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`]+)`").expect("invalid inline code regex"));
static IMAGE_MD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("invalid image md regex"));
static LINK_MD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").expect("invalid link md regex"));
static BR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").expect("invalid br regex"));
static HTML_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</?(div|span|p|strong|em|b|i|code|pre)[^>]*>").expect("invalid html regex"));
static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*#{1,6}[ \t]*").expect("invalid heading regex"));
static BLOCKQUOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*>[ \t]?").expect("invalid blockquote regex"));
static UNORDERED_LIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*[-*+][ \t]+").expect("invalid ulist regex"));
static ORDERED_LIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*\d+\.[ \t]+").expect("invalid olist regex"));
static BOLD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*|__(.+?)__").expect("invalid bold regex"));
static ITALIC_STAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*([^*\s][^*]*[^*\s])\*|\*([^*\s])\*").expect("invalid italic star regex"));
static ITALIC_UNDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_([^_\s][^_]*[^_\s])_|_([^_\s])_").expect("invalid italic under regex"));
static TABLE_PIPE_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\|").expect("invalid pipe start regex"));
static TABLE_PIPE_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\|$").expect("invalid pipe end regex"));
static TABLE_PIPE_INNER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]*\|[ \t]*").expect("invalid pipe inner regex"));
static TRAILING_WS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]+\n").expect("invalid trailing ws regex"));
static MULTI_NEWLINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\n{3,}").expect("invalid multi newline regex"));

fn re_replace_all(re: &Regex, text: &str, rep: &str) -> String {
    re.replace_all(text, rep).into_owned()
}

pub fn clean_outbound_text(text: &str, format_markdown: bool) -> String {
    let out = re_replace_all(&THINK_RE, text, "");
    let out = re_replace_all(&ASSISTANT_PREFIX_RE, &out, "");
    let out = out.replace("\r\n", "\n");

    let out = re_replace_all(&FENCED_CODE_RE, &out, "");
    let out = re_replace_all(&INLINE_CODE_RE, &out, "$1");
    let out = re_replace_all(&IMAGE_MD_RE, &out, "\u{56fe}\u{7247}: $2");
    let out = re_replace_all(&LINK_MD_RE, &out, "$1 ($2)");
    let out = re_replace_all(&BR_RE, &out, "\n");
    let mut out = re_replace_all(&HTML_TAG_RE, &out, "");

    if format_markdown {
        out = re_replace_all(&HEADING_RE, &out, "");
        out = re_replace_all(&BLOCKQUOTE_RE, &out, "");
        out = re_replace_all(&UNORDERED_LIST_RE, &out, "- ");
        out = re_replace_all(&ORDERED_LIST_RE, &out, "- ");
        out = re_replace_all(&BOLD_RE, &out, "$1$2");
        out = re_replace_all(&ITALIC_STAR_RE, &out, "$1$2$3");
        out = re_replace_all(&ITALIC_UNDER_RE, &out, "$1$2$3");
        out = re_replace_all(&TABLE_PIPE_START_RE, &out, "");
        out = re_replace_all(&TABLE_PIPE_END_RE, &out, "");
        out = re_replace_all(&TABLE_PIPE_INNER_RE, &out, " | ");
    }

    let out = re_replace_all(&TRAILING_WS_RE, &out, "\n");
    let out = re_replace_all(&MULTI_NEWLINE_RE, &out, "\n\n");
    out.trim().to_string()
}

pub fn split_message(text: &str, limit: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.len() <= limit {
        return vec![trimmed.to_string()];
    }

    let mut pieces = Vec::with_capacity(4);
    let mut remaining = trimmed;

    while remaining.len() > limit {
        let half = limit / 2;
        let cut = remaining
            .rfind("\n\n")
            .filter(|&pos| pos >= half)
            .or_else(|| remaining.rfind('\n').filter(|&pos| pos >= half))
            .or_else(|| remaining.rfind(' ').filter(|&pos| pos >= half))
            .unwrap_or(limit);

        let chunk = remaining[..cut].trim();
        if !chunk.is_empty() {
            pieces.push(chunk.to_string());
        }
        remaining = remaining[cut..].trim();
    }

    if !remaining.is_empty() {
        pieces.push(remaining.to_string());
    }

    pieces
}

pub async fn extract_image_urls(
    bot: &kovi::RuntimeBot,
    message: &kovi::Message,
    http: &reqwest::Client,
    mode: ImageMode,
) -> Vec<String> {
    let segments = message.get("image");
    if segments.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(segments.len());

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

        match mode {
            ImageMode::Url => {
                kovi::log::info!("hermes: image URL passthrough ({})", truncate_url(&url, 80));
                results.push(url);
            }
            ImageMode::Base64 => match download_as_base64(http, &url).await {
                Ok(data_uri) => {
                    kovi::log::info!("hermes: image downloaded as base64 ({} bytes from {})", data_uri.len(), truncate_url(&url, 80));
                    results.push(data_uri);
                }
                Err(e) => {
                    kovi::log::warn!("hermes: failed to download image {}: {e}", truncate_url(&url, 80));
                }
            },
        }
    }

    results
}

fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        url.to_string()
    } else {
        format!("{}...", &url[..max_len])
    }
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

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();

    let mime = if content_type.starts_with("image/") {
        content_type.clone()
    } else {
        "image/png".to_string()
    };

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("read body failed: {e}"))?;

    if bytes.len() > 20 * 1024 * 1024 {
        return Err(format!("image too large: {} bytes", bytes.len()));
    }

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

pub fn build_context_label(is_group: bool, group_id: i64, user_id: i64, sender_name: &str, is_admin: bool) -> String {
    let role = if is_admin { "\u{7ba1}\u{7406}\u{5458}" } else { "\u{666e}\u{901a}\u{7528}\u{6237}" };
    if is_group {
        format!(
            "\u{5f53}\u{524d}\u{6765}\u{81ea} QQ \u{7fa4} {group_id}\u{3002}\u{53d1}\u{9001}\u{8005}: {sender_name} ({user_id}) [{role}]\u{3002}\u{8bf7}\u{6309} QQ \u{804a}\u{5929}\u{98ce}\u{683c}\u{56de}\u{590d}\u{3002}"
        )
    } else {
        format!(
            "\u{5f53}\u{524d}\u{6765}\u{81ea} QQ \u{79c1}\u{804a}\u{7528}\u{6237} {sender_name} ({user_id}) [{role}]\u{3002}\u{8bf7}\u{6309} QQ \u{804a}\u{5929}\u{98ce}\u{683c}\u{56de}\u{590d}\u{3002}"
        )
    }
}

pub fn build_user_prompt(message: &str, context_label: &str) -> String {
    if context_label.is_empty() {
        message.to_string()
    } else {
        format!("{context_label}\n\n{message}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_regexes_compile() {
        assert!(THINK_RE.is_match("\u{1f9e0}thinking\u{1f9e0}"));
        assert!(ASSISTANT_PREFIX_RE.is_match("assistant: hello"));
        assert!(FENCED_CODE_RE.is_match("```python\n"));
        assert!(INLINE_CODE_RE.is_match("`code`"));
        assert!(IMAGE_MD_RE.is_match("![alt](url)"));
        assert!(LINK_MD_RE.is_match("[text](url)"));
        assert!(BR_RE.is_match("<br/>"));
        assert!(HTML_TAG_RE.is_match("<div>"));
        assert!(HEADING_RE.is_match("## Title"));
        assert!(BLOCKQUOTE_RE.is_match("> quote"));
        assert!(UNORDERED_LIST_RE.is_match("- item"));
        assert!(ORDERED_LIST_RE.is_match("1. item"));
        assert!(BOLD_RE.is_match("**bold**"));
        assert!(BOLD_RE.is_match("__bold__"));
        assert!(ITALIC_STAR_RE.is_match("*italic*"));
        assert!(ITALIC_UNDER_RE.is_match("_italic_"));
        assert!(TABLE_PIPE_START_RE.is_match("| a | b |"));
        assert!(TABLE_PIPE_END_RE.is_match("| a | b |"));
        assert!(TABLE_PIPE_INNER_RE.is_match(" | "));
        assert!(TRAILING_WS_RE.is_match("  \n"));
        assert!(MULTI_NEWLINE_RE.is_match("\n\n\n"));
    }

    #[test]
    fn test_clean_bold() {
        assert_eq!(clean_outbound_text("**hello** world", true), "hello world");
        assert_eq!(clean_outbound_text("__hello__ world", true), "hello world");
    }

    #[test]
    fn test_clean_italic() {
        assert_eq!(clean_outbound_text("*hello* world", true), "hello world");
        assert_eq!(clean_outbound_text("_hello_ world", true), "hello world");
    }

    #[test]
    fn test_clean_heading() {
        assert_eq!(clean_outbound_text("## Title\nbody", true), "Title\nbody");
    }

    #[test]
    fn test_clean_code() {
        assert_eq!(clean_outbound_text("some `code` here", true), "some code here");
    }

    #[test]
    fn test_clean_link() {
        assert_eq!(clean_outbound_text("[click](http://x.com)", true), "click (http://x.com)");
    }

    #[test]
    fn test_clean_image() {
        assert_eq!(clean_outbound_text("![alt](http://x.com/img.png)", true), "\u{56fe}\u{7247}: http://x.com/img.png");
    }

    #[test]
    fn test_clean_blockquote() {
        assert_eq!(clean_outbound_text("> quote\ntext", true), "quote\ntext");
    }

    #[test]
    fn test_clean_list() {
        assert_eq!(clean_outbound_text("- item1\n- item2", true), "- item1\n- item2");
        assert_eq!(clean_outbound_text("1. item1\n2. item2", true), "- item1\n2. item2");
    }

    #[test]
    fn test_clean_table() {
        assert_eq!(clean_outbound_text("| a | b |", true), "a | b");
    }

    #[test]
    fn test_clean_think() {
        assert_eq!(clean_outbound_text("\u{1f9e0}internal\u{1f9e0} visible", true), "visible");
    }

    #[test]
    fn test_no_format_markdown() {
        assert_eq!(clean_outbound_text("**bold**", false), "**bold**");
    }

    #[test]
    fn test_split_message() {
        assert_eq!(split_message("short", 100), vec!["short"]);
    }

    #[test]
    fn test_split_message_long() {
        let long = "a".repeat(200);
        let parts = split_message(&long, 100);
        assert!(parts.len() > 1);
        for part in &parts {
            assert!(!part.is_empty());
        }
    }

    #[test]
    fn test_build_context_label_group() {
        let label = build_context_label(true, 12345, 67890, "Alice", true);
        assert!(label.contains("12345"));
        assert!(label.contains("Alice"));
        assert!(label.contains("67890"));
    }

    #[test]
    fn test_build_context_label_private() {
        let label = build_context_label(false, 0, 67890, "Bob", false);
        assert!(label.contains("Bob"));
        assert!(label.contains("67890"));
    }

    #[test]
    fn test_build_user_prompt() {
        assert_eq!(build_user_prompt("hello", ""), "hello");
        assert_eq!(build_user_prompt("hello", "ctx"), "ctx\n\nhello");
    }
}