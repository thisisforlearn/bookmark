// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Authentication: session extractor + login/register/logout handlers.

use askama::Template;
use axum::Extension;
use axum::Form;
use axum::extract::{FromRequestParts, State};
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use bk_core::Id;
use bk_db::{sessions, users};

use crate::AppState;
use crate::security::{self, CsrfToken};
use crate::views::{LoginTpl, RegisterTpl};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: Id,
    pub username: String,
    pub role: Role,
}

pub fn cookie_from_headers(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

/// Resolve the logged-in user from the session cookie, or `None`.
pub fn current_user(state: &AppState, headers: &axum::http::HeaderMap) -> Option<AuthUser> {
    let token = cookie_from_headers(headers, security::session_cookie_name())?;
    let user_id = state
        .db
        .with_conn_sync(|conn| sessions::validate(conn, &token))
        .ok()
        .flatten()?;
    let user = state
        .db
        .with_conn_sync(|conn| users::find_by_id(conn, user_id))
        .ok()
        .flatten()?;
    Some(AuthUser {
        id: user.id,
        username: user.username,
        role: if user.role == "admin" {
            Role::Admin
        } else {
            Role::User
        },
    })
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match current_user(state, &parts.headers) {
            Some(u) => Ok(u),
            None => Err(Redirect::to("/login").into_response()),
        }
    }
}

fn csrf_of(ext: &Option<Extension<CsrfToken>>) -> String {
    ext.as_ref().map(|e| e.0.0.clone()).unwrap_or_default()
}

pub async fn get_login(
    State(state): State<AppState>,
    Extension(csrf): Extension<CsrfToken>,
) -> Response {
    let tpl = LoginTpl {
        error: None,
        csrf: csrf.0,
        open_registration: state.cfg.open_registration,
    };
    render(tpl.render())
}

pub async fn get_register(
    State(state): State<AppState>,
    Extension(csrf): Extension<CsrfToken>,
) -> Response {
    if !state.cfg.open_registration {
        return Redirect::to("/login").into_response();
    }
    let tpl = RegisterTpl {
        error: None,
        csrf: csrf.0,
    };
    render(tpl.render())
}

fn render(t: Result<String, askama::Error>) -> Response {
    match t {
        Ok(html) => Html(html).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("template error: {e}"),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub csrf: String,
}

pub async fn post_login(
    State(state): State<AppState>,
    Extension(csrf): Extension<CsrfToken>,
    headers: axum::http::HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    if !security::verify_csrf(Some(&csrf), &form.csrf) {
        return (axum::http::StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let found = state
        .db
        .with_conn_sync(|conn| users::find_by_username(conn, &form.username))
        .ok()
        .flatten();
    let (user_id, hash, _role) = match found {
        Some(v) => v,
        None => return login_error(&state, &csrf, "Wrong username or password."),
    };
    let ok = users::verify_password(&hash, &form.password).unwrap_or(false);
    if !ok {
        return login_error(&state, &csrf, "Wrong username or password.");
    }
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let token = match state
        .db
        .with_conn_sync(|conn| sessions::create(conn, user_id, ip.as_deref(), None))
    {
        Ok(t) => t,
        Err(e) => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };
    let mut resp = Redirect::to("/").into_response();
    resp.headers_mut().append(
        axum::http::header::SET_COOKIE,
        security::set_cookie(
            security::session_cookie_name(),
            &token,
            Some(30 * 86_400),
            true,
        ),
    );
    resp
}

fn login_error(state: &AppState, csrf: &CsrfToken, msg: &str) -> Response {
    let tpl = LoginTpl {
        error: Some(msg.to_string()),
        csrf: csrf.0.clone(),
        open_registration: state.cfg.open_registration,
    };
    let mut resp = render(tpl.render());
    *resp.status_mut() = axum::http::StatusCode::UNAUTHORIZED;
    resp
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub csrf: String,
}

pub async fn post_register(
    State(state): State<AppState>,
    Extension(csrf): Extension<CsrfToken>,
    Form(form): Form<RegisterForm>,
) -> Response {
    if !security::verify_csrf(Some(&csrf), &form.csrf) {
        return (axum::http::StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    if !state.cfg.open_registration {
        return (axum::http::StatusCode::FORBIDDEN, "registration is closed").into_response();
    }
    if form.password.len() < 8 {
        let tpl = RegisterTpl {
            error: Some("Password must be at least 8 characters.".into()),
            csrf: csrf.0,
        };
        let mut resp = render(tpl.render());
        *resp.status_mut() = axum::http::StatusCode::BAD_REQUEST;
        return resp;
    }
    match state
        .db
        .with_conn_sync(|conn| users::create(conn, &form.username, &form.password, false))
    {
        Ok(_) => Redirect::to("/login").into_response(),
        Err(e) => {
            let tpl = RegisterTpl {
                error: Some(e.user_message().to_string()),
                csrf: csrf.0,
            };
            let mut resp = render(tpl.render());
            *resp.status_mut() = axum::http::StatusCode::CONFLICT;
            resp
        }
    }
}

pub async fn post_logout(
    State(state): State<AppState>,
    Extension(csrf): Extension<CsrfToken>,
    headers: axum::http::HeaderMap,
    Form(form): Form<StdForm>,
) -> Response {
    if security::verify_csrf(Some(&csrf), &form.csrf)
        && let Some(token) = cookie_from_headers(&headers, security::session_cookie_name())
    {
        let _ = state
            .db
            .with_conn_sync(|conn| sessions::destroy(conn, &token));
    }
    let mut resp = Redirect::to("/login").into_response();
    resp.headers_mut().append(
        axum::http::header::SET_COOKIE,
        security::clear_cookie(security::session_cookie_name()),
    );
    resp
}

#[derive(Deserialize)]
pub struct StdForm {
    #[serde(default)]
    pub csrf: String,
}

#[allow(dead_code)]
fn _unused(ext: &Option<Extension<CsrfToken>>) -> String {
    csrf_of(ext)
}
