// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Query parsing and normalisation.
//!
//! Supported operators: `"quoted phrase"`, `tag:foo`, `site:example.com`,
//! `is:screenshot`, `before:YYYY-MM-DD`, `after:YYYY-MM-DD`, `-negated`.

use bk_core::Result;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedQuery {
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    pub negated: Vec<String>,
    pub tags: Vec<String>,
    pub sites: Vec<String>,
    pub screenshots_only: bool,
    pub before: Option<i64>,
    pub after: Option<i64>,
}

impl ParsedQuery {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.phrases.is_empty()
            && self.tags.is_empty()
            && self.sites.is_empty()
            && !self.screenshots_only
    }

    /// The text used for spell correction (non-operator words).
    pub fn free_text(&self) -> String {
        let mut s = String::new();
        for t in &self.terms {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(t);
        }
        s
    }
}

/// Normalise a single token: lowercase, strip surrounding punctuation.
pub fn normalize_token(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

pub fn parse(input: &str) -> Result<ParsedQuery> {
    let mut q = ParsedQuery::default();
    let mut chars = input.char_indices().peekable();
    let mut buf = String::new();
    let mut negate_next = false;

    let flush = |buf: &mut String, q: &mut ParsedQuery, negate: bool| {
        let tok = std::mem::take(buf);
        if tok.is_empty() {
            return;
        }
        classify(&tok, q, negate);
    };

    while let Some((_, c)) = chars.next() {
        match c {
            '"' => {
                // Read until the closing quote.
                let mut phrase = String::new();
                for (_, ch) in chars.by_ref() {
                    if ch == '"' {
                        break;
                    }
                    phrase.push(ch);
                }
                let p = phrase.trim().to_lowercase();
                if !p.is_empty() {
                    if negate_next {
                        q.negated.push(p);
                        negate_next = false;
                    } else {
                        q.phrases.push(p);
                    }
                }
            }
            '-' if buf.is_empty() => {
                negate_next = true;
            }
            c if c.is_whitespace() => {
                flush(&mut buf, &mut q, negate_next);
                negate_next = false;
            }
            c => buf.push(c),
        }
    }
    flush(&mut buf, &mut q, negate_next);
    Ok(q)
}

fn classify(tok: &str, q: &mut ParsedQuery, negate: bool) {
    let lower = tok.to_lowercase();
    if let Some(v) = lower.strip_prefix("tag:") {
        if !v.is_empty() {
            q.tags.push(normalize_token(v));
        }
        return;
    }
    if let Some(v) = lower.strip_prefix("site:") {
        if !v.is_empty() {
            q.sites.push(v.trim_start_matches("www.").to_string());
        }
        return;
    }
    if lower == "is:screenshot" || lower == "is:image" {
        q.screenshots_only = true;
        return;
    }
    if let Some(v) = lower.strip_prefix("before:") {
        if let Some(ts) = parse_date(v) {
            q.before = Some(ts);
        }
        return;
    }
    if let Some(v) = lower.strip_prefix("after:") {
        if let Some(ts) = parse_date(v) {
            q.after = Some(ts);
        }
        return;
    }
    if negate {
        q.negated.push(normalize_token(&lower));
    } else {
        let t = normalize_token(&lower);
        if !t.is_empty() {
            q.terms.push(t);
        }
    }
}

/// Parse `YYYY-MM-DD` into a unix timestamp (UTC midnight). Tiny, dependency-free.
fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m as u32, d as u32) * 86_400)
}

/// Howard Hinnant's days_from_civil.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((m + 9) % 12) as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_terms() {
        let q = parse("Rust Async").unwrap();
        assert_eq!(q.terms, vec!["rust", "async"]);
    }

    #[test]
    fn operators() {
        let q = parse(
            r#"tag:work site:example.com is:screenshot "hello world" -draft after:2024-01-01"#,
        )
        .unwrap();
        assert_eq!(q.tags, vec!["work"]);
        assert_eq!(q.sites, vec!["example.com"]);
        assert!(q.screenshots_only);
        assert_eq!(q.phrases, vec!["hello world"]);
        assert_eq!(q.negated, vec!["draft"]);
        assert_eq!(q.after, Some(1_704_067_200));
    }

    #[test]
    fn date_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(parse_date("1970-01-02"), Some(86_400));
    }

    #[test]
    fn unclosed_quote_does_not_panic() {
        let q = parse("\"unclosed phrase").unwrap();
        assert_eq!(q.phrases, vec!["unclosed phrase"]);
    }
}
