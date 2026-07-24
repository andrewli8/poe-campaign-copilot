//! Bug-report URL construction: prefills the GitHub issue form with the
//! newest diagnostics-log lines via query parameters.
//!
//! Nothing here performs network I/O — the app only opens the user's
//! browser at the returned URL, and nothing leaves the machine until the
//! user reviews the prefilled form and clicks Submit on GitHub. That
//! preserves the app's "no network during play" contract while removing
//! the manual find-the-log-file-and-drag-it-in step that most reports
//! were skipping.
//!
//! GitHub issue forms accept `?template=<file>&<field-id>=<value>` query
//! parameters; `details` below is the id of the free-text field in
//! `.github/ISSUE_TEMPLATE/bug_report.yml`. URLs have practical length
//! limits (~8 KB across browsers and GitHub), so the log tail is selected
//! newest-line-first against a percent-ENCODED byte budget, and anything
//! older is dropped behind an explicit marker.

/// GitHub Issues page opened by the tray "Report a bug" item. Targets the
/// bug-report issue form (`.github/ISSUE_TEMPLATE/bug_report.yml`) so the
/// user lands on a structured form rather than a blank issue.
const REPORT_BUG_BASE_URL: &str =
    "https://github.com/andrewli8/poe-campaign-copilot/issues/new?template=bug_report.yml";

/// Budget for the percent-encoded `details` query VALUE. The base URL plus
/// the wrapper text around the log add well under 1 KB encoded, keeping
/// the whole URL comfortably below the ~8 KB practical browser/GitHub
/// limit with margin for future fields.
const ENCODED_DETAILS_BUDGET: usize = 5_500;

/// How much raw log to read from the end of the diagnostics file before
/// budget selection. Generous on purpose: the encoded-budget selection in
/// `select_tail_lines` is what actually bounds the URL; this only bounds
/// the read.
pub const LOG_TAIL_BYTES: u64 = 16 * 1024;

/// The full bug-report URL for the tray item: the plain issue form when
/// there is no usable log (`None`, empty, or whitespace-only), otherwise
/// the form with the `details` field prefilled with the newest log lines
/// that fit the encoded budget.
pub fn bug_report_url(version: &str, log_tail: Option<&str>) -> String {
    let Some(log) = log_tail else {
        return REPORT_BUG_BASE_URL.to_string();
    };
    if log.trim().is_empty() {
        return REPORT_BUG_BASE_URL.to_string();
    }
    let (selected, omitted) = select_tail_lines(log, ENCODED_DETAILS_BUDGET);
    let details = details_text(version, &selected, omitted);
    format!("{REPORT_BUG_BASE_URL}&details={}", percent_encode(&details))
}

/// The human-visible prefill for the issue form's `details` textarea. The
/// leading comment is the consent affordance: the user sees exactly what
/// will be posted and is told to redact — log lines carry absolute paths,
/// which on Windows usually include the account username.
fn details_text(version: &str, log_lines: &str, omitted: bool) -> String {
    let marker = if omitted {
        "… (older lines omitted)\n"
    } else {
        ""
    };
    format!(
        "<!-- Review before posting: remove anything you don't want public. \
         Log lines can include file paths with your username. -->\n\n\
         App version: {version}\n\n\
         Recent log (newest last):\n\
         ```text\n{marker}{log_lines}\n```"
    )
}

/// Keeps the NEWEST complete lines of `log` whose percent-encoded sizes
/// (plus one encoded newline each) fit within `budget` bytes, returned in
/// original (oldest-first) order. The second value is true when at least
/// one line was dropped. Blank lines are skipped — they'd spend budget on
/// nothing.
fn select_tail_lines(log: &str, budget: usize) -> (String, bool) {
    let mut kept: Vec<&str> = Vec::new();
    let mut spent = 0usize;
    let mut omitted = false;
    let candidates: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    for line in candidates.iter().rev() {
        // +3 for the "%0A" joining/terminating each line in the encoded value.
        let cost = percent_encode(line).len() + 3;
        if spent + cost > budget {
            omitted = true;
            break;
        }
        spent += cost;
        kept.push(line);
    }
    kept.reverse();
    (kept.join("\n"), omitted)
}

/// Percent-encodes `s` for use as a URL query value: RFC 3986 unreserved
/// characters (ALPHA / DIGIT / `-` / `.` / `_` / `~`) pass through,
/// every other byte of the UTF-8 encoding becomes `%XX`. Deliberately
/// hand-rolled (~10 lines) rather than pulling in a crate for one query
/// parameter.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_log_opens_the_plain_form() {
        assert_eq!(bug_report_url("0.1.10", None), REPORT_BUG_BASE_URL);
        assert_eq!(bug_report_url("0.1.10", Some("")), REPORT_BUG_BASE_URL);
        assert_eq!(
            bug_report_url("0.1.10", Some("  \n \n")),
            REPORT_BUG_BASE_URL
        );
    }

    #[test]
    fn prefilled_url_extends_the_base_with_a_details_param() {
        let url = bug_report_url("0.1.10", Some("1753305600 something broke"));
        assert!(url.starts_with(REPORT_BUG_BASE_URL));
        assert!(url.contains("&details="));
    }

    #[test]
    fn percent_encode_escapes_reserved_and_multibyte_chars() {
        assert_eq!(percent_encode("a b&c=d\n"), "a%20b%26c%3Dd%0A");
        assert_eq!(percent_encode("safe-chars_.~AZ09"), "safe-chars_.~AZ09");
        // Multibyte UTF-8 encodes per byte ("é" = 0xC3 0xA9).
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    #[test]
    fn details_value_contains_no_raw_reserved_chars() {
        let url = bug_report_url("0.1.10", Some("path C:\\Users\\someone & log=x?\n"));
        let details = url.split("&details=").nth(1).expect("details param");
        for forbidden in ['&', '=', '?', '#', ' ', '\n'] {
            assert!(
                !details.contains(forbidden),
                "raw {forbidden:?} in encoded details"
            );
        }
    }

    #[test]
    fn prefill_carries_version_marker_and_fenced_log() {
        let details = details_text("0.1.10", "1 hello\n2 world", false);
        assert!(details.contains("App version: 0.1.10"));
        assert!(details.contains("```text\n1 hello\n2 world\n```"));
        assert!(details.contains("Review before posting"));
        assert!(!details.contains("older lines omitted"));

        let truncated = details_text("0.1.10", "2 world", true);
        assert!(truncated.contains("… (older lines omitted)\n2 world"));
    }

    #[test]
    fn selection_keeps_the_newest_lines_when_over_budget() {
        let log: String = (0..500)
            .map(|i| format!("line-{i:04} something happened here\n"))
            .collect();
        let (selected, omitted) = select_tail_lines(&log, 1_000);
        assert!(omitted, "500 lines cannot fit a 1000-byte budget");
        assert!(selected.ends_with("line-0499 something happened here"));
        assert!(!selected.contains("line-0000"));
        // Chronological order preserved among the kept lines.
        let kept: Vec<&str> = selected.lines().collect();
        assert!(kept.len() > 1, "budget fits more than one line");
        assert!(kept[0] < kept[kept.len() - 1]);
    }

    #[test]
    fn selection_keeps_everything_when_under_budget() {
        let (selected, omitted) = select_tail_lines("1 a\n\n2 b\n", 1_000);
        assert!(!omitted);
        // Blank line dropped; both real lines kept in order.
        assert_eq!(selected, "1 a\n2 b");
    }

    #[test]
    fn url_length_stays_bounded_for_a_huge_log() {
        let log: String = (0..5_000)
            .map(|i| format!("1753305600 line {i} with some typical diagnostic text\n"))
            .collect();
        let url = bug_report_url("0.1.10", Some(&log));
        assert!(
            url.len() <= 7_000,
            "URL must stay under practical browser limits, got {}",
            url.len()
        );
        // The newest line always survives selection.
        assert!(url.contains(&percent_encode("line 4999")));
    }
}
