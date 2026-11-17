// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Charset detection and decoding.

use encoding_rs::{Encoding, UTF_8};

/// Pick a decoder from a `Content-Type` charset parameter, a BOM, or a
/// conservative fallback to UTF-8 (then windows-1252 if invalid).
pub fn detect(content_type: Option<&str>, body: &[u8]) -> &'static Encoding {
    if let Some(ct) = content_type {
        for part in ct.split(';') {
            let part = part.trim();
            if let Some(label) = part.strip_prefix("charset=") {
                let label = label.trim_matches('"');
                if let Some(enc) = Encoding::for_label(label.as_bytes()) {
                    return enc;
                }
            }
        }
    }
    if let Some((enc, _)) = Encoding::for_bom(body) {
        return enc;
    }
    UTF_8
}

/// Decode `body` to a `String`. Never fails: invalid sequences become U+FFFD,
/// and if UTF-8 decoding produces lots of replacements we retry as windows-1252.
pub fn decode(content_type: Option<&str>, body: &[u8]) -> String {
    let enc = detect(content_type, body);
    let (cow, _, _) = enc.decode(body);
    if enc != UTF_8 {
        return cow.into_owned();
    }
    // UTF-8 was assumed; if it's very lossy, fall back to windows-1252.
    let replacements = cow.matches('\u{FFFD}').count();
    if replacements > 8 && replacements * 50 > cow.chars().count() {
        let (win, _, _) = encoding_rs::WINDOWS_1252.decode(body);
        return win.into_owned();
    }
    cow.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_passthrough() {
        assert_eq!(
            decode(Some("text/html; charset=utf-8"), "héllo".as_bytes()),
            "héllo"
        );
    }

    #[test]
    fn respects_declared_charset() {
        // "é" in windows-1252 is 0xE9.
        let bytes = [0xE9u8];
        assert_eq!(decode(Some("text/html; charset=windows-1252"), &bytes), "é");
    }

    #[test]
    fn bom_wins() {
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice(b"hi");
        assert_eq!(decode(None, &b), "hi");
    }
}
