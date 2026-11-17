// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! URL canonicalization. `url_norm` is the dedupe key: two URLs that normalize
//! to the same string are the same bookmark.

use url::Url;

use bk_core::{Error, Result};

/// Query parameters that never change the resource identity.
const TRACKING: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "utm_id",
    "utm_name",
    "fbclid",
    "gclid",
    "dclid",
    "msclkid",
    "mc_cid",
    "mc_eid",
    "igshid",
    "ref",
    "ref_src",
    "spm",
    "yclid",
    "_hsenc",
    "_hsmi",
];

/// True if `raw` looks like an absolute http(s) URL.
pub fn is_http_url(raw: &str) -> bool {
    matches!(Url::parse(raw), Ok(u) if matches!(u.scheme(), "http" | "https"))
}

/// Parse and canonicalize a URL: lowercase scheme+host, drop tracking params,
/// drop the fragment, drop default ports. `Url` already IDNA-encodes the host.
pub fn normalize(raw: &str) -> Result<Url> {
    let mut url = Url::parse(raw).map_err(|e| Error::Validation(format!("invalid url: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Validation("only http(s) URLs are supported".into()));
    }
    // Drop the fragment: it never identifies a different document for us.
    url.set_fragment(None);

    // Drop tracking parameters, preserving order of the rest.
    if url.query().is_some() {
        let kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| !TRACKING.iter().any(|t| k.eq_ignore_ascii_case(t)))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if kept.is_empty() {
            url.set_query(None);
        } else {
            let mut pairs = url.query_pairs_mut();
            pairs.clear();
            for (k, v) in &kept {
                pairs.append_pair(k, v);
            }
        }
    }

    // Strip a trailing empty query `?`.
    if url.query() == Some("") {
        url.set_query(None);
    }
    Ok(url)
}

/// Canonical string used as the `url_norm` column.
pub fn url_norm(raw: &str) -> Result<String> {
    normalize(raw).map(|u| u.to_string())
}

/// Canonical key for a text bookmark (no URL).
pub fn text_norm(text: &str) -> String {
    format!("text:{:016x}", bk_core::xxh3_str(text))
}

/// Host (registrable-ish) used for the `site` column.
pub fn site_of(url: &Url) -> String {
    url.host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tracking_and_fragment() {
        let n = url_norm("HTTPS://Example.COM:443/Path?a=1&utm_source=x&b=2#frag").unwrap();
        assert_eq!(n, "https://example.com/Path?a=1&b=2");
    }

    #[test]
    fn drops_default_ports_and_empty_query() {
        assert_eq!(
            url_norm("http://example.com:80/?utm_medium=y").unwrap(),
            "http://example.com/"
        );
    }

    #[test]
    fn rejects_non_http() {
        assert!(url_norm("ftp://example.com/x").is_err());
        assert!(url_norm("not a url").is_err());
    }

    #[test]
    fn idna() {
        let n = url_norm("https://bücher.example/").unwrap();
        assert_eq!(n, "https://xn--bcher-kva.example/");
    }

    #[test]
    fn site() {
        let u = normalize("https://www.example.com/a").unwrap();
        assert_eq!(site_of(&u), "example.com");
    }

    #[test]
    fn text_key_is_stable() {
        assert_eq!(text_norm("hello"), text_norm("hello"));
        assert_ne!(text_norm("hello"), text_norm("world"));
    }
}
