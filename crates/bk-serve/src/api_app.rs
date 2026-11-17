// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! App-facing API: device pairing, ingest (text + image), and model download.

use std::sync::Arc;

use askama::Template;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::{Extension, Form};
use serde::Deserialize;

use bk_core::Id;

use crate::auth::AuthUser;
use crate::security::CsrfToken;
use crate::{AppError, AppState, ingest, media, tokens};

// ---------------------------------------------------------------- pairing ---

pub async fn pair_page(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
) -> Response {
    let code = match state
        .db
        .with_conn_sync(|c| bk_db::pair_codes::create(c, user.id))
    {
        Ok(c) => c,
        Err(e) => return AppError(e).into_response(),
    };
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:8080")
        .to_string();
    let tpl = PairTpl {
        csrf: csrf.0,
        code,
        host,
        username: user.username,
    };
    match tpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("template: {e}")).into_response(),
    }
}

#[derive(Template)]
#[template(path = "pair.html")]
struct PairTpl {
    csrf: String,
    code: String,
    host: String,
    username: String,
}

#[derive(Deserialize)]
pub struct PairForm {
    pub code: String,
    #[serde(default)]
    pub name: String,
}

/// POST /api/v1/pair — exchange a one-time code for a device token.
pub async fn pair(State(state): State<AppState>, Form(form): Form<PairForm>) -> Response {
    let user_id = match state
        .db
        .with_conn_sync(|c| bk_db::pair_codes::consume(c, &form.code))
    {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid or expired code" })),
            )
                .into_response();
        }
    };
    let name = if form.name.trim().is_empty() {
        "phone"
    } else {
        form.name.trim()
    };
    match state
        .db
        .with_conn_sync(|c| bk_db::devices::create(c, user_id, name))
    {
        Ok(token) => {
            let user = state
                .db
                .with_conn_sync(|c| bk_db::users::find_by_id(c, user_id))
                .ok()
                .flatten()
                .map(|u| u.username)
                .unwrap_or_default();
            Json(serde_json::json!({ "token": token, "user": user })).into_response()
        }
        Err(e) => AppError(e).into_response(),
    }
}

// ------------------------------------------------------------------ auth ---

/// GET /api/v1/whoami — validates a bearer token.
pub async fn whoami(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match tokens::any_user(&state, &headers) {
        Some(u) => Json(serde_json::json!({
            "user": u.username,
            "id": u.id.inner(),
            "role": match u.role { crate::auth::Role::Admin => "admin", _ => "user" },
        }))
        .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response(),
    }
}

// ------------------------------------------------------------------ save ---

/// POST /api/v1/save — multipart ingest of a url, text, or image.
pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let user = match tokens::any_user(&state, &headers) {
        Some(u) => u,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "unauthorized" })),
            )
                .into_response();
        }
    };

    let mut url: Option<String> = None;
    let mut text: Option<String> = None;
    let mut note: Option<String> = None;
    let mut tags_raw: Option<String> = None;
    let mut image: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "url" => url = field.text().await.ok(),
            "text" => text = field.text().await.ok(),
            "note" => note = field.text().await.ok(),
            "tags" => tags_raw = field.text().await.ok(),
            "image" => {
                if let Ok(bytes) = field.bytes().await {
                    if bytes.len() > state.cfg.max_screenshot_bytes {
                        return (StatusCode::PAYLOAD_TOO_LARGE, "image too large").into_response();
                    }
                    image = Some(bytes.to_vec());
                }
            }
            _ => {}
        }
    }

    let tag_list: Vec<String> = tags_raw
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let state = Arc::new(state);
    let result = if let Some(bytes) = image {
        let caption = note.clone().or_else(|| text.clone());
        media::create_screenshot(&state, user.id, &bytes, caption.as_deref(), &tag_list).await
    } else {
        let target = url.or(text).unwrap_or_default();
        if target.trim().is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "nothing to save" })),
            )
                .into_response();
        }
        ingest::create(&state, user.id, &target, &tag_list).await
    };

    match result {
        Ok(id) => {
            state.search.refresh_vocab(user.id);
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "id": id.inner() })),
            )
                .into_response()
        }
        Err(e) => {
            let status = StatusCode::from_u16(e.status()).unwrap_or(StatusCode::BAD_REQUEST);
            (
                status,
                Json(serde_json::json!({ "error": e.user_message() })),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------- models ---

const MODEL_NAMES: [&str; 3] = ["spell", "embed", "rank"];

/// GET /api/v1/models — what the server has, for nerd mode's update button.
pub async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if tokens::any_user(&state, &headers).is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    let mut out = Vec::new();
    for name in MODEL_NAMES {
        let path = state.cfg.models_dir.join(format!("{name}.bkmdl"));
        if let Ok(meta) = std::fs::metadata(&path) {
            out.push(serde_json::json!({
                "name": name,
                "size": meta.len(),
                "url": format!("/api/v1/models/{name}"),
            }));
        }
    }
    Json(serde_json::json!({ "models": out })).into_response()
}

/// GET /api/v1/models/{name} — raw `.bkmdl` bytes.
pub async fn model_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if tokens::any_user(&state, &headers).is_none() {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    if !MODEL_NAMES.contains(&name.as_str()) {
        return (StatusCode::NOT_FOUND, "unknown model").into_response();
    }
    let path = state.cfg.models_dir.join(format!("{name}.bkmdl"));
    match std::fs::read(&path) {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{name}.bkmdl\""),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "model not installed").into_response(),
    }
}

#[allow(dead_code)]
fn _id(v: i64) -> Id {
    Id::from_i64(v)
}
