// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Bookmark creation + the background crawl/embed pipeline.

use std::sync::Arc;

use bk_core::{Error, Id, Result};
use bk_crawl::{CrawlError, FetchOutcome, is_http_url, site_of, text_norm, url_norm};
use bk_db::{bookmarks, crawls, tags, vectors};
use bk_infer::Scratch;

use crate::AppState;

/// Create a bookmark from a URL or free text. Spawns a crawl for URLs.
pub async fn create(
    state: &Arc<AppState>,
    user_id: Id,
    target: &str,
    tag_list: &[String],
) -> Result<Id> {
    let target = target.trim();
    if target.is_empty() {
        return Err(Error::Validation("nothing to save".into()));
    }
    if target.len() > 1_000_000 {
        return Err(Error::Validation("input too large".into()));
    }

    let (url_norm, url, title, text, _site) = if is_http_url(target) {
        let norm = url_norm(target).map_err(|e| Error::Validation(e.to_string()))?;
        let parsed = bk_crawl::normalize(target).map_err(|e| Error::Validation(e.to_string()))?;
        let site = site_of(&parsed);
        (norm, target.to_string(), String::new(), String::new(), site)
    } else {
        // Text bookmark: title is the first non-empty line, capped.
        let first_line = target
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let title: String = first_line.chars().take(120).collect();
        (
            text_norm(target),
            String::new(),
            title,
            target.to_string(),
            String::new(),
        )
    };

    let exists = state
        .db
        .with_conn_sync(|conn| bookmarks::get_by_url(conn, user_id, &url_norm))?;
    if exists.is_some() {
        return Err(Error::Conflict("bookmark already exists".into()));
    }

    let id = state
        .db
        .with_conn_sync(|conn| bookmarks::insert(conn, user_id, &url_norm, &url, &title, &text))?;

    if !tag_list.is_empty() {
        let tags = tag_list.to_vec();
        state
            .db
            .with_conn_sync(|conn| tags::set(conn, user_id, id, &tags))?;
    }

    // Text bookmarks can be embedded immediately; URL bookmarks after crawl.
    if !text.is_empty() {
        embed_bookmark(state, id).await;
    } else {
        let st = state.clone();
        let target = url.clone();
        tokio::spawn(async move {
            crawl_bookmark(&st, id, &target).await;
        });
    }
    Ok(id)
}

/// Fetch a URL, apply the extracted text, then embed.
pub async fn crawl_bookmark(state: &Arc<AppState>, id: Id, url: &str) {
    let parsed = match bk_crawl::normalize(url) {
        Ok(u) => u,
        Err(_) => return,
    };
    let outcome = state.fetcher.fetch(&parsed).await;
    match outcome {
        Ok(FetchOutcome::Article(a)) => {
            let title = if a.title.is_empty() {
                url.to_string()
            } else {
                a.title
            };
            let text: String = a.text.chars().take(state.cfg.max_text_bytes).collect();
            let site = a.site.clone();
            let lang = a.lang.clone();
            let desc = a.description.clone();
            let quality = bk_crawl::quality_score(text.len(), a.boilerplate_ratio);
            let _ = state
                .db
                .exec(move |tx| {
                    bookmarks::apply_crawl(
                        tx,
                        id,
                        &title,
                        &desc,
                        &text,
                        lang.as_deref(),
                        &site,
                        quality,
                    )?;
                    crawls::record(tx, id, Some(200), None, text.len() as i64, 0)?;
                    Ok(())
                })
                .await;
            embed_bookmark(state, id).await;
        }
        Ok(FetchOutcome::Binary { mime, .. }) => {
            let kind = format!("binary:{mime}");
            let _ = state
                .db
                .exec(move |tx| {
                    bookmarks::set_status(tx, id, "binary")?;
                    crawls::record(tx, id, Some(200), Some(&kind), 0, 0)?;
                    Ok(())
                })
                .await;
        }
        Err(e) => {
            let kind = e.kind().to_string();
            let retryable = e.retryable();
            let status = match &e {
                CrawlError::Status(s) => Some(*s),
                _ => None,
            };
            let _ = state
                .db
                .exec(move |tx| {
                    let st = if retryable { "retry" } else { "failed" };
                    bookmarks::set_status(tx, id, st)?;
                    crawls::record(tx, id, status, Some(&kind), 0, 0)?;
                    Ok(())
                })
                .await;
        }
    }
}

/// Compute and store an int8 embedding for a bookmark, if a model is present.
pub async fn embed_bookmark(state: &Arc<AppState>, id: Id) {
    let model = match &state.models.embed {
        Some(m) => m.clone(),
        None => return,
    };
    let row = match state.db.with_conn_sync(|conn| bookmarks::get_raw(conn, id)) {
        Ok(Some(b)) => b,
        _ => return,
    };
    let combined = format!("{} {}", row.title, row.text);
    let combined: String = combined.chars().take(4000).collect();
    let mut scratch = Scratch::new();
    let mut buf = Vec::new();
    if let Ok(scale) = model.encode(&combined, &mut buf, &mut scratch) {
        let data: Vec<i8> = buf.iter().map(|&b| b as i8).collect();
        let _ = state
            .db
            .exec(move |tx| vectors::put(tx, id, "embed", data.len(), scale, &data))
            .await;
        state.search.invalidate_vectors();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_url_vs_text() {
        assert!(is_http_url("https://example.com/a"));
        assert!(!is_http_url("just some text"));
        assert_eq!(text_norm("hello"), text_norm("hello"));
    }
}
