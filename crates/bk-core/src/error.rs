// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Typed error type. Every failure path in the server goes through this enum so
//! callers get a stable `kind()` string and a stable HTTP status code.

/// The error type used throughout the server.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(String),
    #[error("http: {0}")]
    Http(String),
    #[error("model: {0}")]
    Model(String),
    #[error("crawl: {0}")]
    Crawl(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("rate limited")]
    RateLimited,
    #[error("internal: {0}")]
    Internal(String),
}

/// Result alias.
pub type Result<T> = core::result::Result<T, Error>;

impl Error {
    /// Stable HTTP status code for this error.
    #[inline]
    pub fn status(&self) -> u16 {
        match self {
            Error::Validation(_) | Error::Conflict(_) => 409,
            Error::NotFound => 404,
            Error::Unauthorized => 401,
            Error::Forbidden | Error::RateLimited => 403,
            _ => 500,
        }
    }

    /// Stable, machine-readable string for metrics/audit logs.
    #[inline]
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Db(_) => "db",
            Error::Http(_) => "http",
            Error::Model(_) => "model",
            Error::Crawl(_) => "crawl",
            Error::Io(_) => "io",
            Error::NotFound => "not_found",
            Error::Conflict(_) => "conflict",
            Error::Validation(_) => "validation",
            Error::Unauthorized => "unauthorized",
            Error::Forbidden => "forbidden",
            Error::RateLimited => "rate_limited",
            Error::Internal(_) => "internal",
        }
    }

    /// A short, user-facing one-line message. Never contains internal detail.
    #[inline]
    pub fn user_message(&self) -> &'static str {
        match self {
            Error::Validation(_) => "That input didn't look right. Please try again.",
            Error::Conflict(_) => "That item already exists.",
            Error::NotFound => "Not found.",
            Error::Unauthorized => "Please sign in.",
            Error::Forbidden => "You don't have permission to do that.",
            Error::RateLimited => "Too many requests. Please wait a moment.",
            Error::Db(_) => "The database is busy. Please try again.",
            Error::Model(_) => "The search model is unavailable; showing plain search results.",
            Error::Crawl(_) => "We couldn't fetch that page.",
            Error::Http(_) => "A network error occurred.",
            Error::Io(_) => "A disk error occurred.",
            Error::Internal(_) => "Something went wrong on the server.",
        }
    }
}

#[cfg(feature = "db")]
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
            rusqlite::Error::SqliteFailure(inner, _)
                if inner.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Error::Conflict("already exists".into())
            }
            other => Error::Db(other.to_string()),
        }
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Internal(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Internal(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_kinds() {
        assert_eq!(Error::NotFound.status(), 404);
        assert_eq!(Error::Validation("x".into()).status(), 409);
        assert_eq!(Error::Forbidden.status(), 403);
        assert_eq!(Error::RateLimited.status(), 403);
        assert_eq!(Error::Db("x".into()).status(), 500);
        assert_eq!(Error::NotFound.kind(), "not_found");
        assert_eq!(Error::RateLimited.kind(), "rate_limited");
    }
}
