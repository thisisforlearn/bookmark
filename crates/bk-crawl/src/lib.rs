// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Light, polite, single-page crawler. No JavaScript, no deep crawl.

pub mod charset;
pub mod fetch;
pub mod readability;
pub mod robots;
pub mod ssrf;
pub mod url;

pub use fetch::{Fetcher, download};
pub use readability::Extracted;
pub use url::{is_http_url, normalize, site_of, text_norm, url_norm};

/// A successfully extracted page.
#[derive(Clone, Debug, Default)]
pub struct Article {
    pub url: String,
    pub final_url: String,
    pub site: String,
    pub title: String,
    pub description: String,
    pub text: String,
    pub lang: Option<String>,
    pub text_hash: u64,
    pub mime: Option<String>,
    pub boilerplate_ratio: f32,
}

/// A typed crawl failure. Each variant maps to a user-visible message and a
/// retry policy; nothing here is a bare string from a lower layer.
#[derive(Clone, Debug, thiserror::Error)]
pub enum CrawlError {
    #[error("dns: {0}")]
    Dns(String),
    #[error("tls: {0}")]
    Tls(String),
    #[error("http status {0}")]
    Status(u16),
    #[error("rate limited")]
    TooManyRequests,
    #[error("timeout")]
    Timeout,
    #[error("response too large ({0} bytes)")]
    TooLarge(u64),
    #[error("empty body")]
    Empty,
    #[error("unknown charset")]
    UnknownCharset,
    #[error("not html ({0})")]
    NotHtml(String),
    #[error("blocked by robots.txt")]
    Robots,
    #[error("blocked: {0}")]
    Ssrf(String),
    #[error("too many redirects")]
    RedirectLoop,
    #[error("io: {0}")]
    Io(String),
}

impl CrawlError {
    /// Stable machine-readable kind, used for the `crawls.error_kind` column.
    pub fn kind(&self) -> &'static str {
        match self {
            CrawlError::Dns(_) => "dns",
            CrawlError::Tls(_) => "tls",
            CrawlError::Status(_) => "status",
            CrawlError::TooManyRequests => "rate_limited",
            CrawlError::Timeout => "timeout",
            CrawlError::TooLarge(_) => "too_large",
            CrawlError::Empty => "empty",
            CrawlError::UnknownCharset => "charset",
            CrawlError::NotHtml(_) => "not_html",
            CrawlError::Robots => "robots",
            CrawlError::Ssrf(_) => "blocked",
            CrawlError::RedirectLoop => "redirect_loop",
            CrawlError::Io(_) => "io",
        }
    }

    /// Whether the caller should retry later.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            CrawlError::Timeout
                | CrawlError::TooManyRequests
                | CrawlError::Io(_)
                | CrawlError::Dns(_)
                | CrawlError::Status(429 | 500 | 502 | 503 | 504)
        )
    }

    /// A short message safe to show a user.
    pub fn user_message(&self) -> &'static str {
        match self {
            CrawlError::Dns(_) => "The site's address couldn't be resolved.",
            CrawlError::Tls(_) => "The site's certificate couldn't be verified.",
            CrawlError::Status(404) => "The page was not found.",
            CrawlError::Status(403) => "The site refused access.",
            CrawlError::Status(_) => "The site returned an error.",
            CrawlError::TooManyRequests => "The site asked us to slow down.",
            CrawlError::Timeout => "The site took too long to respond.",
            CrawlError::TooLarge(_) => "The page is too large to index.",
            CrawlError::Empty => "The page was empty.",
            CrawlError::UnknownCharset => "The page's text encoding wasn't recognised.",
            CrawlError::NotHtml(_) => "That link isn't a web page.",
            CrawlError::Robots => "The site's robots.txt disallows crawling.",
            CrawlError::Ssrf(_) => "That address isn't allowed.",
            CrawlError::RedirectLoop => "The site redirected too many times.",
            CrawlError::Io(_) => "A network error occurred.",
        }
    }
}

/// Heuristic 0..1 quality score for an extracted page. Long pages with a high
/// (kept / total) text ratio score well; thin, chrome-heavy pages score low.
pub fn quality_score(text_len: usize, boilerplate_ratio: f32) -> f32 {
    let length = (text_len as f32 / 1200.0).clamp(0.0, 1.0);
    let density = boilerplate_ratio.clamp(0.0, 1.0);
    (0.65 * length + 0.35 * density).clamp(0.0, 1.0)
}

/// What a fetch produced.
#[derive(Clone, Debug)]
pub enum FetchOutcome {
    /// An HTML page that was extracted into an article.
    Article(Article),
    /// A non-HTML resource (PDF, image, …). We store only the URL and mime.
    Binary { mime: String, bytes: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_heuristic() {
        assert!(quality_score(4000, 1.0) > 0.9);
        assert!(quality_score(120, 0.1) < 0.15);
        assert!((0.0..=1.0).contains(&quality_score(0, 0.0)));
    }

    #[test]
    fn retry_policy() {
        assert!(CrawlError::Timeout.retryable());
        assert!(CrawlError::Status(503).retryable());
        assert!(!CrawlError::Status(404).retryable());
        assert!(!CrawlError::Robots.retryable());
        assert_eq!(CrawlError::Status(429).kind(), "status");
    }
}
