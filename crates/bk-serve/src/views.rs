// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Askama view models. These mirror the templates in `templates/` exactly.

use askama::Template;

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTpl {
    pub error: Option<String>,
    pub csrf: String,
    pub open_registration: bool,
}

#[derive(Template)]
#[template(path = "register.html")]
pub struct RegisterTpl {
    pub error: Option<String>,
    pub csrf: String,
}

#[derive(Clone, Debug, Default)]
pub struct TagView {
    pub name: String,
    pub count: i64,
    pub active: bool,
}

#[derive(Clone, Debug, Default)]
pub struct HitView {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub site: String,
    pub path: String,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub has_shot: bool,
    pub low_quality: bool,
    pub initial: String,
}

#[derive(Template)]
#[template(path = "list.html")]
pub struct ListTpl {
    pub csrf: String,
    pub query: String,
    pub hits: Vec<HitView>,
    pub total: i64,
    pub took_ms: u32,
    pub corrected: Option<String>,
    pub degraded: bool,
    pub tags: Vec<TagView>,
    pub page: i64,
    pub has_next: bool,
}

#[derive(Template)]
#[template(path = "results.html")]
pub struct ResultsTpl {
    pub query: String,
    pub hits: Vec<HitView>,
    pub total: i64,
    pub took_ms: u32,
    pub corrected: Option<String>,
    pub degraded: bool,
    pub tags: Vec<TagView>,
    pub page: i64,
    pub has_next: bool,
}

#[derive(Clone, Debug, Default)]
pub struct BmView {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub text: String,
    pub note: String,
    pub tags: Vec<String>,
    pub tags_joined: String,
    pub pinned: bool,
    pub has_shot: bool,
    pub created_at_str: String,
    pub status: String,
    pub site: String,
    pub initial: String,
}

#[derive(Clone, Debug)]
pub struct ShareView {
    pub id: i64,
    pub token: String,
}

#[derive(Template)]
#[template(path = "detail.html")]
pub struct DetailTpl {
    pub csrf: String,
    pub bm: BmView,
    pub shares: Vec<ShareView>,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsTpl {
    pub csrf: String,
    pub username: String,
    pub role: String,
    pub bookmark_count: i64,
    pub tag_count: i64,
    pub models: Vec<(String, bool)>,
    pub open_registration: bool,
}

#[derive(Template)]
#[template(path = "share.html")]
pub struct ShareTpl {
    pub bm: BmView,
    pub token: String,
}

#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorTpl {
    pub code: u16,
    pub message: String,
}

/// Format a unix timestamp as `YYYY-MM-DD`.
pub fn date_str(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn initial_of(title: &str, url: &str) -> String {
    let src = if !title.is_empty() { title } else { url };
    src.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "•".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date_str(0), "1970-01-01");
        assert_eq!(date_str(1_704_067_200), "2024-01-01");
    }

    #[test]
    fn initials() {
        assert_eq!(initial_of("hello", ""), "H");
        assert_eq!(initial_of("", "https://x.com"), "H");
        assert_eq!(initial_of("", ""), "•");
    }

    #[test]
    fn templates_compile() {
        let t = ErrorTpl {
            code: 404,
            message: "x".into(),
        };
        assert!(t.render().unwrap().contains("404"));
    }
}
