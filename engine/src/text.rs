//! Small shared text-boundary helpers for bounded metadata and previews.

/// Truncate at a valid UTF-8 boundary while keeping the total output within
/// `max_bytes`. The ellipsis is part of that byte limit when truncation occurs.
pub(crate) fn truncate_utf8_bytes(value: &str, max_bytes: usize, ellipsis: &str) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    if ellipsis.len() > max_bytes {
        return String::new();
    }

    let prefix_limit = max_bytes - ellipsis.len();
    let boundary = value
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= prefix_limit)
        .last()
        .unwrap_or(0);
    let mut truncated = String::with_capacity(boundary + ellipsis.len());
    truncated.push_str(&value[..boundary]);
    truncated.push_str(ellipsis);
    truncated
}

#[cfg(test)]
mod tests {
    use super::truncate_utf8_bytes;

    #[test]
    fn ascii_below_exact_and_above_byte_limit() {
        assert_eq!(truncate_utf8_bytes("abc", 4, "..."), "abc");
        assert_eq!(truncate_utf8_bytes("abc", 3, "..."), "abc");
        assert_eq!(truncate_utf8_bytes("abcde", 4, "..."), "a...");
    }

    #[test]
    fn chinese_crossing_boundary_keeps_valid_prefix() {
        let result = truncate_utf8_bytes("中文abc", 8, "...");
        assert_eq!(result, "中...");
        assert!(result.len() <= 8);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn mixed_chinese_and_ascii_is_deterministic() {
        let first = truncate_utf8_bytes("中a界b", 7, "...");
        let second = truncate_utf8_bytes("中a界b", 7, "...");
        assert_eq!(first, "中a...");
        assert_eq!(first, second);
    }

    #[test]
    fn emoji_and_multi_codepoint_sequences_never_split_bytes() {
        let result = truncate_utf8_bytes("😀👩‍💻abc", 7, "...");
        assert_eq!(result, "😀...");
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn undersized_limits_and_empty_values_are_safe() {
        assert_eq!(truncate_utf8_bytes("中文", 0, "..."), "");
        assert_eq!(truncate_utf8_bytes("中文", 1, "..."), "");
        assert_eq!(truncate_utf8_bytes("中文", 2, "..."), "");
        assert_eq!(truncate_utf8_bytes("中文", 3, "..."), "...");
        assert_eq!(truncate_utf8_bytes("", 0, "..."), "");
    }
}
