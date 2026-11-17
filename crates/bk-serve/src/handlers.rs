// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! HTTP handlers.

use std::sync::Arc;

use askama::Template;
use axum::Extension;
use axum::Form;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Json, Redirect, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use bk_core::{Error, Id};
use bk_db::{bookmarks, shares, tags, users};
use bk_search::{SearchRequest, SearchResponse};

use crate::AppState;
use crate::auth::{self, AuthUser};
use crate::ingest;
use crate::security::{self, CsrfToken};
use crate::views::{
    DetailTpl, ErrorTpl, HitView, ListTpl, ResultsTpl, SettingsTpl, ShareTpl, ShareView, date_str,
    initial_of,
};

const PAGE: i64 = 30;

fn render_html(t: Result<String, askama::Error>) -> Response {
    match t {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("template: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response()
        }
    }
}

fn submitted_csrf(headers: &HeaderMap, form: &str) -> String {
    if !form.is_empty() {
        return form.to_string();
    }
    headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

fn unauthorized_json() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error":"unauthorized"})),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct IndexQuery {
    pub q: Option<String>,
    pub page: Option<i64>,
    pub partial: Option<i64>,
}

/// `GET /` and `GET /?partial=1` (JSON-free HTML fragment for live search).
pub async fn index(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    Query(q): Query<IndexQuery>,
) -> Response {
    let query = q.q.unwrap_or_default();
    let page = q.page.unwrap_or(0).max(0);
    let partial = q.partial.unwrap_or(0) == 1;

    let (hits, total, took_ms, corrected, degraded) = if query.trim().is_empty() {
        let rows = state
            .db
            .with_conn_sync(|conn| {
                bookmarks::list(conn, user.id, PAGE as usize, (page * PAGE) as usize)
            })
            .unwrap_or_default();
        let total = state
            .db
            .with_conn_sync(|conn| bookmarks::count(conn, user.id))
            .unwrap_or(0);
        let shot = shots_for(&state, &rows);
        let hv = rows
            .iter()
            .map(|b| to_hit(state.clone(), b, "", shot.contains(&b.id)))
            .collect();
        (hv, total, 1u32, None, state.models.degraded())
    } else {
        let req = SearchRequest {
            q: query.clone(),
            user_id: user.id,
            limit: PAGE as usize,
            offset: (page * PAGE) as usize,
        };
        let searcher = state.search.clone();
        let resp = tokio::task::spawn_blocking(move || searcher.search(&req))
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        let hv = search_hits_to_views(state.clone(), &resp, user.id);
        (
            hv,
            resp.candidates as i64,
            resp.took_ms,
            resp.corrected_query.clone(),
            resp.degraded,
        )
    };

    let raw_tags = state
        .db
        .with_conn_sync(|conn| tags::list(conn, user.id))
        .unwrap_or_default();
    let filter_tag = query
        .trim()
        .strip_prefix("tag:")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    let tag_views: Vec<crate::views::TagView> = raw_tags
        .into_iter()
        .map(|(name, count)| crate::views::TagView {
            active: !filter_tag.is_empty() && name.to_lowercase() == filter_tag,
            name,
            count,
        })
        .collect();
    let has_next = (page + 1) * PAGE < total;

    if partial {
        let tpl = ResultsTpl {
            query,
            hits,
            total,
            took_ms,
            corrected,
            degraded,
            tags: tag_views,
            page,
            has_next,
        };
        return render_html(tpl.render());
    }
    let tpl = ListTpl {
        csrf: csrf.0,
        query,
        hits,
        total,
        took_ms,
        corrected,
        degraded,
        tags: tag_views,
        page,
        has_next,
    };
    render_html(tpl.render())
}

fn shots_for(
    state: &AppState,
    rows: &[bk_db::bookmarks::Bookmark],
) -> std::collections::HashSet<Id> {
    let ids: Vec<Id> = rows.iter().map(|b| b.id).collect();
    state
        .db
        .with_conn_sync(|conn| bk_db::screenshots::existing_ids(conn, &ids))
        .unwrap_or_default()
}

fn to_hit(
    state: AppState,
    b: &bk_db::bookmarks::Bookmark,
    snippet: &str,
    has_shot: bool,
) -> HitView {
    let path = if b.site.is_empty() {
        b.url.clone()
    } else {
        String::new()
    };
    let tag_list = state
        .db
        .with_conn_sync(|conn| tags::for_bookmark(conn, b.id))
        .unwrap_or_default();
    HitView {
        id: b.id.inner(),
        url: b.url.clone(),
        title: if b.title.is_empty() {
            b.url.clone()
        } else {
            b.title.clone()
        },
        snippet: snippet.to_string(),
        site: b.site.clone(),
        path,
        tags: tag_list,
        pinned: b.pinned,
        has_shot,
        low_quality: b.quality < 0.35,
        initial: initial_of(&b.title, &b.url),
    }
}

fn search_hits_to_views(state: AppState, resp: &SearchResponse, _user: Id) -> Vec<HitView> {
    let ids: Vec<Id> = resp.hits.iter().map(|h| h.bookmark.id).collect();
    let shots = state
        .db
        .with_conn_sync(|conn| bk_db::screenshots::existing_ids(conn, &ids))
        .unwrap_or_default();
    resp.hits
        .iter()
        .map(|h| {
            let b = &h.bookmark;
            HitView {
                id: b.id.inner(),
                url: b.url.clone(),
                title: if b.title.is_empty() {
                    b.url.clone()
                } else {
                    b.title.clone()
                },
                snippet: h.snippet.clone(),
                site: b.site.clone(),
                path: String::new(),
                tags: h.tags.clone(),
                pinned: b.pinned,
                has_shot: shots.contains(&b.id),
                low_quality: b.quality < 0.35,
                initial: initial_of(&b.title, &b.url),
            }
        })
        .collect()
}

pub async fn detail(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    Path(id): Path<i64>,
) -> Response {
    let bm = match state
        .db
        .with_conn_sync(|conn| bookmarks::get(conn, user.id, Id::from_i64(id)))
    {
        Ok(Some(b)) => b,
        _ => return not_found().await,
    };
    let tags = state
        .db
        .with_conn_sync(|conn| tags::for_bookmark(conn, bm.id))
        .unwrap_or_default();
    let shares = state
        .db
        .with_conn_sync(|conn| shares::list_for_bookmark(conn, user.id, bm.id))
        .unwrap_or_default()
        .into_iter()
        .map(|s| ShareView {
            id: s.id.inner(),
            token: s.token,
        })
        .collect();
    let has_shot = state
        .db
        .with_conn_sync(|conn| bk_db::screenshots::exists(conn, bm.id))
        .unwrap_or(false);
    let view = crate::views::BmView {
        id: bm.id.inner(),
        url: bm.url.clone(),
        title: if bm.title.is_empty() {
            bm.url.clone()
        } else {
            bm.title.clone()
        },
        text: bm.text.clone(),
        note: bm.note.clone(),
        tags_joined: tags.join(", "),
        tags,
        pinned: bm.pinned,
        has_shot,
        created_at_str: date_str(bm.created_at),
        status: bm.status.clone(),
        site: bm.site.clone(),
        initial: initial_of(&bm.title, &bm.url),
    };
    let tpl = DetailTpl {
        csrf: csrf.0,
        bm: view,
        shares,
    };
    render_html(tpl.render())
}

pub async fn screenshot(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    let id = Id::from_i64(id);
    let owned = state
        .db
        .with_conn_sync(|conn| bookmarks::get(conn, user.id, id))
        .ok()
        .flatten()
        .is_some();
    if !owned {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    match state
        .db
        .with_conn_sync(|conn| bk_db::screenshots::get(conn, id))
    {
        Ok(Some(s)) => {
            let etag = format!("\"{}\"", s.sha256);
            (
                [
                    (header::CONTENT_TYPE, s.mime.clone()),
                    (header::ETAG, etag),
                    (
                        header::CACHE_CONTROL,
                        "private, max-age=31536000, immutable".to_string(),
                    ),
                ],
                s.blob,
            )
                .into_response()
        }
        _ => (StatusCode::NOT_FOUND, "no screenshot").into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateForm {
    pub target: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub csrf: String,
}

pub async fn create_bookmark(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
    Form(form): Form<CreateForm>,
) -> Response {
    let token = submitted_csrf(&headers, &form.csrf);
    if !security::verify_csrf(Some(&csrf), &token) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let tag_list: Vec<String> = form
        .tags
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let state = Arc::new(state);
    match ingest::create(&state, user.id, &form.target, &tag_list).await {
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

pub async fn delete_bookmark(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    let token = submitted_csrf(&headers, "");
    if !security::verify_csrf(Some(&csrf), &token) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    match state
        .db
        .with_conn_sync(|conn| bookmarks::delete(conn, user.id, Id::from_i64(id)))
    {
        Ok(true) => {
            state.search.refresh_vocab(user.id);
            StatusCode::NO_CONTENT.into_response()
        }
        _ => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[derive(Deserialize)]
pub struct PinBody {
    pub pinned: bool,
}

pub async fn pin_bookmark(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<PinBody>,
) -> Response {
    let token = submitted_csrf(&headers, "");
    if !security::verify_csrf(Some(&csrf), &token) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    match state
        .db
        .with_conn_sync(|conn| bookmarks::set_pinned(conn, user.id, Id::from_i64(id), body.pinned))
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[derive(Deserialize)]
pub struct TagsForm {
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub csrf: String,
}

pub async fn set_tags(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Form(form): Form<TagsForm>,
) -> Response {
    let token = submitted_csrf(&headers, &form.csrf);
    if !security::verify_csrf(Some(&csrf), &token) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let list: Vec<String> = form
        .tags
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let _ = state
        .db
        .with_conn_sync(|conn| tags::set(conn, user.id, Id::from_i64(id), &list));
    state.search.refresh_vocab(user.id);
    Redirect::to(&format!("/b/{id}")).into_response()
}

pub async fn upload_screenshot(
    State(state): State<AppState>,
    _user: AuthUser,
    Extension(expected): Extension<CsrfToken>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Response {
    let id = Id::from_i64(id);
    let owned = state
        .db
        .with_conn_sync(|conn| bookmarks::get(conn, _user.id, id))
        .ok()
        .flatten()
        .is_some();
    if !owned {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let mut csrf_seen: Option<String> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        if name == "csrf" {
            csrf_seen = field.text().await.ok();
            continue;
        }
        if name != "image" {
            continue;
        }
        let data = match field.bytes().await {
            Ok(b) => b,
            Err(_) => return (StatusCode::BAD_REQUEST, "bad upload").into_response(),
        };
        if let Some(tok) = &csrf_seen {
            if !security::verify_csrf(Some(&expected), tok) {
                return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
            }
        } else {
            return (StatusCode::FORBIDDEN, "missing csrf token").into_response();
        }
        if data.len() > state.cfg.max_screenshot_bytes {
            return (StatusCode::PAYLOAD_TOO_LARGE, "image too large").into_response();
        }
        match store_screenshot(&state, id, &data) {
            Ok(()) => return Redirect::to(&format!("/b/{id}")).into_response(),
            Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
        }
    }
    (StatusCode::BAD_REQUEST, "no image field").into_response()
}

fn store_screenshot(state: &AppState, id: Id, data: &[u8]) -> Result<(), String> {
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
    let mime = "image/png";
    let mut hasher = Sha256::new();
    hasher.update(&blob);
    let sha = {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    };
    state
        .db
        .with_conn_sync(|conn| bk_db::screenshots::put(conn, id, mime, w, h, &sha, &blob))
        .map_err(|e| e.to_string())
}

pub async fn create_share(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Form(form): Form<security::EmptyForm>,
) -> Response {
    let token = submitted_csrf(&headers, &form.csrf);
    if !security::verify_csrf(Some(&csrf), &token) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let bookmark = Id::from_i64(id);
    let owned = state
        .db
        .with_conn_sync(|conn| bookmarks::get(conn, user.id, bookmark))
        .ok()
        .flatten()
        .is_some();
    if !owned {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let _ = state
        .db
        .with_conn_sync(|conn| shares::create(conn, user.id, bookmark, None));
    Redirect::to(&format!("/b/{id}")).into_response()
}

pub async fn revoke_share(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    let token = submitted_csrf(&headers, "");
    if !security::verify_csrf(Some(&csrf), &token) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let _ = state
        .db
        .with_conn_sync(|conn| shares::revoke(conn, user.id, Id::from_i64(id)));
    Redirect::to("/settings").into_response()
}

pub async fn settings(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
) -> Response {
    let bookmark_count = state
        .db
        .with_conn_sync(|conn| bookmarks::count(conn, user.id))
        .unwrap_or(0);
    let tag_count = state
        .db
        .with_conn_sync(|conn| tags::list(conn, user.id))
        .map(|v| v.len() as i64)
        .unwrap_or(0);
    let models = vec![
        ("spell".to_string(), state.models.spell.is_some()),
        ("embed".to_string(), state.models.embed.is_some()),
        ("rank".to_string(), state.models.rank.is_some()),
    ];
    let tpl = SettingsTpl {
        csrf: csrf.0,
        username: user.username,
        role: if user.role == auth::Role::Admin {
            "admin".into()
        } else {
            "user".into()
        },
        bookmark_count,
        tag_count,
        models,
        open_registration: state.cfg.open_registration,
    };
    render_html(tpl.render())
}

#[derive(Deserialize)]
pub struct PasswordForm {
    pub current: String,
    pub new: String,
    #[serde(default)]
    pub csrf: String,
}

pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    Extension(csrf): Extension<CsrfToken>,
    headers: HeaderMap,
    Form(form): Form<PasswordForm>,
) -> Response {
    let token = submitted_csrf(&headers, &form.csrf);
    if !security::verify_csrf(Some(&csrf), &token) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let found = state
        .db
        .with_conn_sync(|conn| users::find_by_username(conn, &user.username))
        .ok()
        .flatten();
    if let Some((_, hash, _)) = found
        && users::verify_password(&hash, &form.current).unwrap_or(false)
        && form.new.len() >= 8
    {
        let _ = state
            .db
            .with_conn_sync(|conn| users::set_password(conn, user.id, &form.new));
    }
    Redirect::to("/settings").into_response()
}

pub async fn healthz(State(state): State<AppState>) -> Response {
    let db_ok = state
        .db
        .with_conn_sync(|c| Ok(c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?))
        .is_ok();
    let body = serde_json::json!({
        "ok": db_ok,
        "uptime_s": state.started.elapsed().as_secs(),
        "models": {
            "spell": state.models.spell.is_some(),
            "embed": state.models.embed.is_some(),
            "rank": state.models.rank.is_some(),
        }
    });
    let code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body)).into_response()
}

pub async fn public_share(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let share = match state
        .db
        .with_conn_sync(|conn| shares::resolve(conn, &token))
    {
        Ok(Some(s)) => s,
        _ => return not_found().await,
    };
    let bm = match state
        .db
        .with_conn_sync(|conn| bookmarks::get_raw(conn, share.bookmark_id))
    {
        Ok(Some(b)) => b,
        _ => return not_found().await,
    };
    let tag_list = state
        .db
        .with_conn_sync(|conn| tags::for_bookmark(conn, bm.id))
        .unwrap_or_default();
    let has_shot = state
        .db
        .with_conn_sync(|conn| bk_db::screenshots::exists(conn, bm.id))
        .unwrap_or(false);
    let view = crate::views::BmView {
        id: bm.id.inner(),
        url: bm.url.clone(),
        title: if bm.title.is_empty() {
            bm.url.clone()
        } else {
            bm.title.clone()
        },
        text: bm.text.chars().take(20_000).collect(),
        note: String::new(),
        tags_joined: tag_list.join(", "),
        tags: tag_list,
        pinned: false,
        has_shot,
        created_at_str: date_str(bm.created_at),
        status: bm.status.clone(),
        site: bm.site.clone(),
        initial: initial_of(&bm.title, &bm.url),
    };
    let tpl = ShareTpl { bm: view, token };
    render_html(tpl.render())
}

pub async fn public_share_screenshot(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    let share = match state
        .db
        .with_conn_sync(|conn| shares::resolve(conn, &token))
    {
        Ok(Some(s)) => s,
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    match state
        .db
        .with_conn_sync(|conn| bk_db::screenshots::get(conn, share.bookmark_id))
    {
        Ok(Some(s)) => ([(header::CONTENT_TYPE, s.mime)], s.blob).into_response(),
        _ => (StatusCode::NOT_FOUND, "no screenshot").into_response(),
    }
}

#[derive(Deserialize)]
pub struct ApiSearchQuery {
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Serialize)]
struct ApiHit {
    id: i64,
    url: String,
    title: String,
    site: String,
    snippet: String,
    tags: Vec<String>,
    pinned: bool,
}

pub async fn api_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ApiSearchQuery>,
) -> Response {
    let user = match crate::tokens::any_user(&state, &headers) {
        Some(u) => u,
        None => return unauthorized_json(),
    };
    let req = SearchRequest {
        q: q.q.unwrap_or_default(),
        user_id: user.id,
        limit: q.limit.unwrap_or(30).min(200),
        offset: q.offset.unwrap_or(0),
    };
    let searcher = state.search.clone();
    let resp = match tokio::task::spawn_blocking(move || searcher.search(&req)).await {
        Ok(Ok(r)) => r,
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, "search failed").into_response(),
    };
    let hits: Vec<ApiHit> = resp
        .hits
        .iter()
        .map(|h| ApiHit {
            id: h.bookmark.id.inner(),
            url: h.bookmark.url.clone(),
            title: h.bookmark.title.clone(),
            site: h.bookmark.site.clone(),
            snippet: h.snippet.clone(),
            tags: h.tags.clone(),
            pinned: h.bookmark.pinned,
        })
        .collect();
    Json(serde_json::json!({
        "hits": hits,
        "total": resp.candidates,
        "took_ms": resp.took_ms,
        "corrected_query": resp.corrected_query,
        "degraded": resp.degraded,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct ShareForm {
    pub target: String,
    #[serde(default)]
    pub tags: String,
}

/// Save a bookmark from the Android share sheet.
pub async fn api_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ShareForm>,
) -> Response {
    let user = match crate::tokens::any_user(&state, &headers) {
        Some(u) => u,
        None => return unauthorized_json(),
    };
    let tag_list: Vec<String> = form
        .tags
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let state = Arc::new(state);
    match ingest::create(&state, user.id, &form.target, &tag_list).await {
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

pub async fn api_bookmarks(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match crate::tokens::any_user(&state, &headers) {
        Some(u) => u,
        None => return unauthorized_json(),
    };
    let rows = state
        .db
        .with_conn_sync(|conn| bookmarks::list(conn, user.id, 100, 0))
        .unwrap_or_default();
    Json(
        rows.into_iter()
            .map(|b| ApiHit {
                id: b.id.inner(),
                url: b.url.clone(),
                title: b.title.clone(),
                site: b.site.clone(),
                snippet: String::new(),
                tags: vec![],
                pinned: b.pinned,
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub async fn not_found() -> Response {
    let tpl = ErrorTpl {
        code: 404,
        message: "That page doesn't exist.".into(),
    };
    let mut resp = render_html(tpl.render());
    *resp.status_mut() = StatusCode::NOT_FOUND;
    resp
}

#[allow(dead_code)]
fn _err(m: &str) -> Error {
    Error::Validation(m.to_string())
}
