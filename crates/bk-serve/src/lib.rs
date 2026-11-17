// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! The HTTP server: application state, router and static assets.

pub mod auth;
pub mod handlers;
pub mod ingest;
pub mod security;
pub mod views;

use std::sync::Arc;
use std::time::Instant;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Router, middleware};
use tower_http::trace::TraceLayer;

use bk_core::{Config, Error, Result};
use bk_crawl::Fetcher;
use bk_db::Db;
use bk_infer::ModelSet;
use bk_search::{SearchConfig, Searcher};

use crate::security::RateLimiter;

const APP_CSS: &str = include_str!("../assets/app.css");
const APP_JS: &str = include_str!("../assets/app.js");
const FAVICON: &str = include_str!("../assets/favicon.svg");

/// Shared application state. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub cfg: Arc<Config>,
    pub search: Arc<Searcher>,
    pub models: Arc<ModelSet>,
    pub fetcher: Arc<Fetcher>,
    pub limiter: Arc<RateLimiter>,
    pub started: Instant,
}

/// Build the state from config: open the DB, load models, wire search + crawler.
pub fn build_state(cfg: Config) -> Result<AppState> {
    let db = Db::open(&cfg.db_path())?;
    let models = Arc::new(ModelSet::load(&cfg.models_dir));
    let search_cfg = SearchConfig {
        spell: cfg.search_spell,
        semantic: cfg.search_semantic,
        rerank: cfg.search_rerank,
        ..SearchConfig::default()
    };
    let search = Searcher::new(db.clone(), models.clone(), search_cfg);
    let fetcher = Arc::new(
        Fetcher::new(
            cfg.crawl_concurrency,
            cfg.crawl_polite_ms,
            cfg.max_text_bytes,
        )
        .map_err(|e| Error::Crawl(e.to_string()))?,
    );
    let limiter = Arc::new(RateLimiter::new(cfg.rate_limit_rps));
    Ok(AppState {
        db,
        cfg: Arc::new(cfg),
        search,
        models,
        fetcher,
        limiter,
        started: Instant::now(),
    })
}

/// Build the router with all routes and middleware.
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/api/v1/search", get(handlers::api_search))
        .route("/api/v1/bookmarks", get(handlers::api_bookmarks));

    let authed = Router::new()
        .route("/", get(handlers::index))
        .route("/b/{id}", get(handlers::detail))
        .route("/screenshot/{id}", get(handlers::screenshot))
        .route("/bookmarks", post(handlers::create_bookmark))
        .route("/bookmarks/{id}", delete(handlers::delete_bookmark))
        .route("/bookmarks/{id}/pin", post(handlers::pin_bookmark))
        .route("/bookmarks/{id}/tags", post(handlers::set_tags))
        .route(
            "/bookmarks/{id}/screenshot",
            post(handlers::upload_screenshot),
        )
        .route("/bookmarks/{id}/share", post(handlers::create_share))
        .route("/shares/{id}/revoke", post(handlers::revoke_share))
        .route("/settings", get(handlers::settings))
        .route("/settings/password", post(handlers::change_password))
        .merge(api);

    Router::new()
        .route("/login", get(auth::get_login).post(auth::post_login))
        .route(
            "/register",
            get(auth::get_register).post(auth::post_register),
        )
        .route("/logout", post(auth::post_logout))
        .route("/healthz", get(handlers::healthz))
        .route("/share/{token}", get(handlers::public_share))
        .route(
            "/share/{token}/screenshot",
            get(handlers::public_share_screenshot),
        )
        .route("/static/app.css", get(static_css))
        .route("/static/app.js", get(static_js))
        .route("/static/favicon.svg", get(static_favicon))
        .merge(authed)
        .fallback(handlers::not_found)
        .layer(middleware::from_fn(security::csrf_middleware))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::rate_limit_middleware,
        ))
        .layer(middleware::from_fn(security::security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn static_css() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        APP_CSS,
    )
}

async fn static_js() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        APP_JS,
    )
}

async fn static_favicon() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "image/svg+xml")],
        FAVICON,
    )
}

/// Convert a `bk_core::Error` into an HTTP response.
pub struct AppError(pub Error);

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        tracing::warn!(kind = self.0.kind(), "request failed: {}", self.0);
        (status, self.0.user_message()).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_nonempty() {
        assert!(APP_CSS.contains("--accent"));
        assert!(APP_JS.contains("liveSearch"));
        assert!(FAVICON.contains("<svg"));
    }
}
