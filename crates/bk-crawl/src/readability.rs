// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Readability-style extraction: strip chrome, pick the main content container
//! by paragraph text density, and return clean text + metadata.

use lol_html::{RewriteStrSettings, element, rewrite_str};
use scraper::{Html, Selector};
use url::Url;

/// The text-bearing tags we keep, in document order.
const TEXT_TAGS: &str = "p, h1, h2, h3, h4, h5, h6, li, pre, blockquote, td, dd";
const NOISE_TAGS: &str =
    "script, style, nav, header, footer, aside, form, noscript, iframe, svg, template";

/// Result of extracting a page.
#[derive(Clone, Debug, Default)]
pub struct Extracted {
    pub title: String,
    pub description: String,
    pub text: String,
    pub lang: Option<String>,
    pub boilerplate_ratio: f32,
}

/// Remove chrome elements before parsing. Falls back to the raw HTML if the
/// rewriter fails (malformed markup), because html5ever is tolerant anyway.
fn strip_noise(html: &str) -> String {
    let settings =
        RewriteStrSettings::new().append_element_content_handler(element!(NOISE_TAGS, |el| {
            el.remove();
            Ok(())
        }));
    rewrite_str(html, settings).unwrap_or_else(|_| html.to_string())
}

fn sel(s: &str) -> Option<Selector> {
    Selector::parse(s).ok()
}

fn text_of(el: &scraper::ElementRef<'_>) -> String {
    let mut out = String::new();
    for t in el.text() {
        out.push_str(t);
    }
    collapse(&out)
}

/// Collapse runs of whitespace to single spaces and trim.
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out.trim().to_string()
}

/// Extract the main article from an HTML document.
pub fn extract(html: &str, _base: &Url, max_bytes: usize) -> Extracted {
    let cleaned = strip_noise(html);
    let doc = Html::parse_document(&cleaned);

    let title = extract_title(&doc);
    let description = extract_description(&doc);

    let text_tags = sel(TEXT_TAGS);
    let body_sel = sel("body");

    // Total paragraph text length across the document (for the boilerplate ratio).
    let total_len = match (&text_tags, &body_sel) {
        (Some(tags), Some(body)) => match doc.select(body).next() {
            Some(b) => b.select(tags).map(|e| text_of(&e).len()).sum::<usize>(),
            None => 0,
        },
        _ => 0,
    };

    let best = pick_container(&doc, total_len);
    let mut parts: Vec<String> = Vec::new();
    if let Some(tags) = &text_tags {
        match &best {
            Some(container) => {
                for el in container.select(tags) {
                    let t = text_of(&el);
                    if t.len() >= 24 || t.split_whitespace().count() >= 4 {
                        parts.push(t);
                    }
                }
            }
            None => {
                for el in doc.select(tags) {
                    let t = text_of(&el);
                    if !t.is_empty() {
                        parts.push(t);
                    }
                }
            }
        }
    }

    let mut text = parts.join("\n\n");
    if text.len() > max_bytes {
        let mut cut = max_bytes;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }

    let kept_len = text.len();
    let boilerplate_ratio = if total_len == 0 {
        0.0
    } else {
        (kept_len as f32 / total_len as f32).clamp(0.0, 1.0)
    };

    let lang = if text.chars().count() > 40 {
        whatlang::detect(&text).map(|i| i.lang().code().to_string())
    } else {
        None
    };

    Extracted {
        title,
        description,
        text,
        lang,
        boilerplate_ratio,
    }
}

fn extract_title(doc: &Html) -> String {
    if let Some(s) = sel("meta[property='og:title']")
        && let Some(el) = doc.select(&s).next()
        && let Some(c) = el.value().attr("content")
    {
        let c = collapse(c);
        if !c.is_empty() {
            return c;
        }
    }
    if let Some(s) = sel("title")
        && let Some(el) = doc.select(&s).next()
    {
        let t = text_of(&el);
        if !t.is_empty() {
            return t;
        }
    }
    if let Some(s) = sel("h1")
        && let Some(el) = doc.select(&s).next()
    {
        let t = text_of(&el);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

fn extract_description(doc: &Html) -> String {
    for selector in [
        "meta[name='description']",
        "meta[property='og:description']",
    ] {
        if let Some(s) = sel(selector)
            && let Some(el) = doc.select(&s).next()
            && let Some(c) = el.value().attr("content")
        {
            let c = collapse(c);
            if !c.is_empty() {
                return c;
            }
        }
    }
    String::new()
}

/// Pick the container with the densest paragraph text.
fn pick_container(doc: &Html, total_len: usize) -> Option<scraper::ElementRef<'_>> {
    let p_sel = sel("p")?;
    let candidate_sel = sel("article, main, [role=main]")?;

    let mut best: Option<(usize, scraper::ElementRef<'_>)> = None;
    for el in doc.select(&candidate_sel) {
        let len: usize = el.select(&p_sel).map(|p| text_of(&p).len()).sum();
        let better = match &best {
            Some((blen, _)) => len > *blen,
            None => true,
        };
        if better {
            best = Some((len, el));
        }
    }
    // Only trust an article/main container if it holds a meaningful share of
    // the document text; otherwise fall back to the whole document.
    match best {
        Some((len, el)) if total_len == 0 || len * 2 >= total_len => Some(el),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/post").unwrap()
    }

    #[test]
    fn strips_nav_and_keeps_article() {
        let html = r#"
        <html><head><title>My Post</title>
        <meta name="description" content="A short summary."></head>
        <body>
          <nav><a href="/">Home</a><a href="/x">X</a></nav>
          <article>
            <h1>My Post</h1>
            <p>This is the first paragraph of the article, long enough to keep.</p>
            <p>This is the second paragraph with more useful words in it.</p>
            <script>alert('noise')</script>
          </article>
          <footer>copyright 2026</footer>
        </body></html>"#;
        let e = extract(html, &base(), 400 * 1024);
        assert_eq!(e.title, "My Post");
        assert_eq!(e.description, "A short summary.");
        assert!(e.text.contains("first paragraph"));
        assert!(!e.text.contains("copyright"));
        assert!(!e.text.contains("alert"));
        assert!(!e.text.contains("Home"));
    }

    #[test]
    fn respects_size_cap() {
        let big = "word ".repeat(100_000);
        let html = format!("<html><body><article><p>{big}</p></article></body></html>");
        let e = extract(&html, &base(), 1000);
        assert!(e.text.len() <= 1001);
    }

    #[test]
    fn never_panics_on_garbage() {
        let e = extract("<html><body><p>unclosed <div><span", &base(), 1000);
        assert!(e.text.len() < 2000);
        let _ = extract("", &base(), 1000);
    }
}
