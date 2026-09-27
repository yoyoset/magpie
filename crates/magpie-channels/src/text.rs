//! Small text helpers shared by channels.

use magpie_core::Severity;

/// Cuts to at most `max_chars` characters, marking the cut with `…`.
pub fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars.saturating_sub(1)) {
        Some((cut, _)) if s.chars().count() > max_chars => format!("{}…", &s[..cut]),
        _ => s.to_owned(),
    }
}

pub fn severity_badge(s: Severity) -> &'static str {
    match s {
        Severity::Info => "⚪",
        Severity::Normal => "🔵",
        Severity::Urgent => "🟠",
        Severity::Critical => "🔴",
    }
}

/// "第 2 次提醒" for repeats, nothing for the first delivery.
pub fn repeat_note(attempt: u32) -> Option<String> {
    (attempt > 0).then(|| format!("第 {} 次提醒", attempt + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_counts_chars_and_marks_cut() {
        assert_eq!(truncate("喜鹊报信", 10), "喜鹊报信");
        assert_eq!(truncate("喜鹊报信", 4), "喜鹊报信");
        assert_eq!(truncate("喜鹊报信啦", 4), "喜鹊报…");
        assert_eq!(truncate("喜鹊报信啦", 4).chars().count(), 4);
    }

    #[test]
    fn repeat_note_only_after_first() {
        assert_eq!(repeat_note(0), None);
        assert_eq!(repeat_note(1).as_deref(), Some("第 2 次提醒"));
    }
}
