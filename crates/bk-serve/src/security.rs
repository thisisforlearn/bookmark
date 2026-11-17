// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! CSRF (double-submit), rate limiting, security headers, and cookie helpers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use rand::RngCore;

use crate::AppState;

/// The CSRF token for the current request, injected by the middleware.
#[derive(Clone, Debug)]
pub struct CsrfToken(pub String);

const CSRF_COOKIE: &str = "bk_csrf";
const SESSION_COOKIE: &str = "bk_session";

pub fn random_token() -> String {
    let mut b = [0u8; 24];
    rand::rng().fill_bytes(&mut b);
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Constant-time-ish comparison that never short-circuits on content.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Read one cookie value from the request headers.
pub fn cookie(req: &Request, name: &str) -> Option<String> {
    let raw = req.headers().get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

pub fn set_cookie(name: &str, value: &str, max_age: Option<i64>, http_only: bool) -> HeaderValue {
    let mut s = format!("{name}={value}; Path=/; SameSite=Lax");
    if http_only {
        s.push_str("; HttpOnly");
    }
    if let Some(age) = max_age {
        s.push_str(&format!("; Max-Age={age}"));
    }
    HeaderValue::from_str(&s).unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn clear_cookie(name: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn session_cookie_name() -> &'static str {
    SESSION_COOKIE
}

/// Ensure every request has a CSRF token. Generates one and sets the cookie if
/// missing, and makes the token available to handlers via request extensions.
pub async fn csrf_middleware(mut req: Request, next: Next) -> Response {
    let existing = cookie(&req, CSRF_COOKIE);
    let token = existing.clone().unwrap_or_else(random_token);
    req.extensions_mut().insert(CsrfToken(token.clone()));
    let mut resp = next.run(req).await;
    if existing.is_none()
        && let Ok(v) =
            HeaderValue::from_str(&format!("{CSRF_COOKIE}={token}; Path=/; SameSite=Lax"))
    {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    resp
}

/// Per-IP token bucket. A burst of `rps` is allowed; the bucket refills at `rps`
/// per second.
pub struct RateLimiter {
    rps: f64,
    buckets: DashMap<String, (f64, Instant)>,
}

impl RateLimiter {
    pub fn new(rps: u32) -> Self {
        Self {
            rps: rps.max(1) as f64,
            buckets: DashMap::new(),
        }
    }

    pub fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut e = self
            .buckets
            .entry(key.to_string())
            .or_insert((self.rps, now));
        let (tokens, last) = &mut *e;
        let elapsed = now.duration_since(*last).as_secs_f64();
        *tokens = (*tokens + elapsed * self.rps).min(self.rps);
        *last = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Drop stale buckets so memory doesn't grow without bound.
    pub fn gc(&self) {
        let now = Instant::now();
        self.buckets
            .retain(|_, (_, last)| now.duration_since(*last) < Duration::from_secs(300));
    }
}

/// Rate-limit middleware keyed by client IP.
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "local".to_string());
    if !state.limiter.allow(&ip) {
        return (StatusCode::TOO_MANY_REQUESTS, "Too many requests").into_response();
    }
    next.run(req).await
}

/// Add hardening headers to every response.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; \
             base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
        ),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    resp
}

/// True for methods that must carry a valid CSRF token.
pub fn needs_csrf(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Validate a submitted CSRF token against the request's token.
pub fn verify_csrf(expected: Option<&CsrfToken>, submitted: &str) -> bool {
    match expected {
        Some(CsrfToken(exp)) => ct_eq(exp, submitted),
        None => false,
    }
}

/// An empty form that only carries the CSRF token.
#[derive(Debug, Default, serde::Deserialize)]
pub struct EmptyForm {
    #[serde(default)]
    pub csrf: String,
}

/// Arc helper for tests.
pub fn shared_limiter(rps: u32) -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(rps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_works() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "ab"));
    }

    #[test]
    fn rate_limiter_bursts_then_blocks() {
        let l = RateLimiter::new(3);
        assert!(l.allow("x"));
        assert!(l.allow("x"));
        assert!(l.allow("x"));
        assert!(!l.allow("x"));
        assert!(l.allow("y"));
    }

    #[test]
    fn csrf_verify() {
        let t = CsrfToken("tok".into());
        assert!(verify_csrf(Some(&t), "tok"));
        assert!(!verify_csrf(Some(&t), "other"));
        assert!(!verify_csrf(None, "tok"));
    }
}
