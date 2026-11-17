// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Progressive Web App: manifest, service worker, icons, and the Android share
//! target. This is what lets a phone "install" bookmark to the home screen and
//! save links straight from any app's Share menu — no APK required.

use axum::http::header::{self, HeaderValue};
use axum::response::{IntoResponse, Json, Redirect, Response};

pub const MANIFEST: &str = r##"{
  "name": "bookmark",
  "short_name": "bookmark",
  "description": "Your personal, self-hosted bookmark library",
  "start_url": "/",
  "scope": "/",
  "display": "standalone",
  "orientation": "portrait-primary",
  "background_color": "#0d0f14",
  "theme_color": "#4f46e5",
  "categories": ["productivity", "utilities"],
  "icons": [
    { "src": "/static/icon-192.png", "sizes": "192x192", "type": "image/png", "purpose": "any" },
    { "src": "/static/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any" },
    { "src": "/static/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" }
  ],
  "share_target": {
    "action": "/share-target",
    "method": "POST",
    "enctype": "multipart/form-data",
    "params": { "title": "title", "text": "text", "url": "url" }
  }
}"##;

pub const SERVICE_WORKER: &str = include_str!("../assets/sw.js");
pub const ICON_192: &[u8] = include_bytes!("../assets/icon-192.png");
pub const ICON_512: &[u8] = include_bytes!("../assets/icon-512.png");
pub const APPLE_TOUCH: &[u8] = include_bytes!("../assets/apple-touch-icon.png");

pub async fn manifest() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/manifest+json"),
        )],
        MANIFEST,
    )
        .into_response()
}

pub async fn service_worker() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/javascript; charset=utf-8"),
            ),
            (
                header::HeaderName::from_static("service-worker-allowed"),
                HeaderValue::from_static("/"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, must-revalidate"),
            ),
        ],
        SERVICE_WORKER,
    )
        .into_response()
}

pub async fn icon_192() -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
        ICON_192,
    )
        .into_response()
}

pub async fn icon_512() -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
        ICON_512,
    )
        .into_response()
}

pub async fn apple_touch() -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
        APPLE_TOUCH,
    )
        .into_response()
}

/// POST /share-target — fallback for browsers that post the share payload here
/// without an active service worker. The service worker normally intercepts
/// this and forwards to /api/v1/share instead.
pub async fn share_target_fallback() -> Redirect {
    Redirect::to("/?share=open")
}

/// JSON form of the manifest for clients that prefer to fetch it.
pub async fn manifest_json() -> Json<serde_json::Value> {
    Json(serde_json::from_str(MANIFEST).unwrap_or_else(|_| serde_json::json!({})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_valid_json() {
        let v: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(v["share_target"]["method"], "POST");
        assert!(v["icons"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn icons_are_png() {
        assert_eq!(&ICON_192[..4], b"\x89PNG");
        assert_eq!(&ICON_512[..4], b"\x89PNG");
    }
}
