// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Image ingest for the Android app: normalise a shared screenshot into a
//! bookmark with an attached image.

use std::sync::Arc;

use sha2::{Digest, Sha256};

use bk_core::{Error, Id, Result};
use bk_db::{bookmarks, screenshots, tags};

use crate::AppState;
use crate::views::date_str;

/// Decode, cap at 1600 px, re-encode as PNG. Returns `(bytes, width, height, sha)`.
pub fn process_image(data: &[u8]) -> std::result::Result<(Vec<u8>, u32, u32, String), String> {
    use image::ImageFormat;
    let img = image::load_from_memory(data).map_err(|e| format!("not an image: {e}"))?;
    let img = if img.width() > 1600 {
        img.resize(1600, 1600, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let (w, h) = (img.width(), img.height());
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .map_err(|e| format!("encode: {e}"))?;
    let blob = out.into_inner();
    let mut hasher = Sha256::new();
    hasher.update(&blob);
    let sha = {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    };
    Ok((blob, w, h, sha))
}

/// Create a bookmark backed by an image (a shared screenshot).
pub async fn create_screenshot(
    state: &Arc<AppState>,
    user_id: Id,
    data: &[u8],
    caption: Option<&str>,
    tag_list: &[String],
) -> Result<Id> {
    let (blob, w, h, sha) = process_image(data).map_err(Error::Validation)?;

    let caption = caption.unwrap_or("").trim();
    let title = if caption.is_empty() {
        format!("Screenshot · {}", date_str(bk_core::now()))
    } else {
        caption.chars().take(120).collect::<String>()
    };
    let target = format!("screenshot:{sha}");
    let norm = bk_crawl::text_norm(&target);

    let id = state
        .db
        .with_conn_sync(|c| bookmarks::insert(c, user_id, &norm, "", &title, ""))?;

    if !tag_list.is_empty() {
        let t = tag_list.to_vec();
        state.db.with_conn_sync(|c| tags::set(c, user_id, id, &t))?;
    }

    let mime = "image/png";
    state
        .db
        .with_conn_sync(|c| screenshots::put(c, id, mime, w, h, &sha, &blob))?;

    Ok(id)
}
