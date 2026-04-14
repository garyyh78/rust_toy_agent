//! text_util.rs - Unicode-safe string truncation utilities
//!
//! All user-facing string truncation should go through `truncate_chars` to
//! avoid panicking on multi-byte (non-ASCII) text.

/// Return a prefix of `s` containing at most `max_chars` Unicode scalar values.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

/// Same as `truncate_chars` but appends "..." if truncated.
pub fn truncate_chars_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push_str("...");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_handles_ascii() {
        assert_eq!(truncate_chars("hello", 3), "hel");
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert_eq!(truncate_chars("", 5), "");
    }

    #[test]
    fn truncate_chars_handles_emoji() {
        assert_eq!(truncate_chars("héllo🚀", 4), "héll");
        assert_eq!(truncate_chars("短い", 10), "短い");
    }

    #[test]
    fn truncate_chars_ellipsis_no_truncation() {
        assert_eq!(truncate_chars_ellipsis("hi", 5), "hi");
    }

    #[test]
    fn truncate_chars_ellipsis_truncated() {
        assert_eq!(truncate_chars_ellipsis("hello world", 5), "hello...");
    }
}
