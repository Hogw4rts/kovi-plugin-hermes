use regex::Regex;
use std::sync::LazyLock;

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
    LazyLock::new(|| Regex::new(r"\*([^*\s](?:[^*]*[^*\s])?)\*").expect("invalid italic star regex"));
static ITALIC_UNDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_([^_\s](?:[^_]*[^_\s])?)_").expect("invalid italic under regex"));
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

pub(crate) fn clean_outbound_text(text: &str, format_markdown: bool) -> String {
    let out = THINK_RE.replace_all(text, "");
    let out = ASSISTANT_PREFIX_RE.replace_all(&out, "");
    let mut out = out.replace("\r\n", "\n");

    out = FENCED_CODE_RE.replace_all(&out, "").into_owned();
    out = INLINE_CODE_RE.replace_all(&out, "$1").into_owned();
    out = IMAGE_MD_RE.replace_all(&out, "图片: $2").into_owned();
    out = LINK_MD_RE.replace_all(&out, "$1 ($2)").into_owned();
    out = BR_RE.replace_all(&out, "\n").into_owned();
    out = HTML_TAG_RE.replace_all(&out, "").into_owned();

    if format_markdown {
        out = HEADING_RE.replace_all(&out, "").into_owned();
        out = BLOCKQUOTE_RE.replace_all(&out, "").into_owned();
        out = UNORDERED_LIST_RE.replace_all(&out, "- ").into_owned();
        out = ORDERED_LIST_RE.replace_all(&out, "- ").into_owned();
        out = BOLD_RE.replace_all(&out, "$1$2").into_owned();
        out = ITALIC_STAR_RE.replace_all(&out, "$1$2$3").into_owned();
        out = ITALIC_UNDER_RE.replace_all(&out, "$1$2$3").into_owned();
        out = TABLE_PIPE_START_RE.replace_all(&out, "").into_owned();
        out = TABLE_PIPE_END_RE.replace_all(&out, "").into_owned();
        out = TABLE_PIPE_INNER_RE.replace_all(&out, " | ").into_owned();
    }

    out = TRAILING_WS_RE.replace_all(&out, "\n").into_owned();
    out = MULTI_NEWLINE_RE.replace_all(&out, "\n\n").into_owned();
    out.trim().to_string()
}

pub(crate) fn split_message(text: &str, limit: usize) -> Vec<String> {
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
        assert!(ITALIC_STAR_RE.is_match("*a*"));
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
        assert_eq!(clean_outbound_text("*a* world", true), "a world");
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
        assert_eq!(clean_outbound_text("![alt](http://x.com/img.png)", true), "图片: http://x.com/img.png");
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
}