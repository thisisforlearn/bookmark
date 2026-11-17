// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! The fetcher: guarded, polite, size-capped single-page HTTP.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use futures::StreamExt;
use reqwest::redirect::Policy;
use tokio::sync::Semaphore;
use url::Url;

use crate::readability;
use crate::robots::RobotsCache;
use crate::ssrf;
use crate::{Article, CrawlError, FetchOutcome, charset, site_of};

const USER_AGENT: &str = "bookmark/0.1 (+https://github.com/thisisforlearn/bookmark)";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 3;
const BODY_CAP: usize = 2 * 1024 * 1024;

/// A configured crawler. Cloning is cheap (`Arc`-backed internals).
pub struct Fetcher {
    client: reqwest::Client,
    sem: Arc<Semaphore>,
    robots: RobotsCache,
    polite: DashMap<String, Instant>,
    polite_ms: u64,
    max_text_bytes: usize,
}

impl Fetcher {
    pub fn new(
        concurrency: usize,
        polite_ms: u64,
        max_text_bytes: usize,
    ) -> Result<Self, CrawlError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.error("too many redirects");
                }
                if ssrf::check(attempt.url()).is_err() {
                    return attempt.error("blocked redirect");
                }
                attempt.follow()
            }))
            .build()
            .map_err(|e| CrawlError::Io(e.to_string()))?;
        Ok(Self {
            client,
            sem: Arc::new(Semaphore::new(concurrency.max(1))),
            robots: RobotsCache::new(),
            polite: DashMap::new(),
            polite_ms,
            max_text_bytes,
        })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Fetch and classify a URL.
    pub async fn fetch(&self, url: &Url) -> Result<FetchOutcome, CrawlError> {
        // 1. SSRF pre-check (DNS is blocking, so run it off the reactor).
        let url_clone = url.clone();
        tokio::task::spawn_blocking(move || ssrf::check(&url_clone))
            .await
            .map_err(|e| CrawlError::Io(e.to_string()))?
            .map_err(|e| CrawlError::Ssrf(e.to_string()))?;

        // 2. robots.txt.
        let path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        if !self.robots.allowed(&self.client, url, path).await {
            return Err(CrawlError::Robots);
        }

        // 3. Politeness: one request per host per `polite_ms`.
        self.wait_for_host(url).await;

        // 4. Concurrency cap.
        let _permit = self
            .sem
            .acquire()
            .await
            .map_err(|e| CrawlError::Io(e.to_string()))?;

        // 5. Request.
        let resp = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(map_reqwest)?;

        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(CrawlError::TooManyRequests);
        }
        if !status.is_success() {
            return Err(CrawlError::Status(status.as_u16()));
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let is_html = match &content_type {
            Some(ct) => {
                let ct = ct.to_ascii_lowercase();
                ct.contains("text/html") || ct.contains("application/xhtml")
            }
            None => true,
        };

        // 6. Bounded body read.
        let (body, nbytes) = read_capped(resp, BODY_CAP).await?;

        if nbytes == 0 {
            return Err(CrawlError::Empty);
        }

        if !is_html {
            return Ok(FetchOutcome::Binary {
                mime: content_type.unwrap_or_else(|| "application/octet-stream".into()),
                bytes: nbytes as u64,
            });
        }

        // 7. Decode + extract.
        let html = charset::decode(content_type.as_deref(), &body);
        if html.trim().is_empty() {
            return Err(CrawlError::Empty);
        }
        let ex = readability::extract(&html, url, self.max_text_bytes);
        let text_hash = bk_core::xxh3_str(&ex.text);

        Ok(FetchOutcome::Article(Article {
            url: url.to_string(),
            final_url: url.to_string(),
            site: site_of(url),
            title: ex.title,
            description: ex.description,
            text: ex.text,
            lang: ex.lang,
            text_hash,
            mime: content_type,
            boilerplate_ratio: ex.boilerplate_ratio,
        }))
    }

    async fn wait_for_host(&self, url: &Url) {
        if self.polite_ms == 0 {
            return;
        }
        let host = url.host_str().unwrap_or_default().to_string();
        let now = Instant::now();
        let wait = {
            let next = self.polite.get(&host).map(|v| *v.value());
            match next {
                Some(next_at) if next_at > now => next_at - now,
                _ => Duration::ZERO,
            }
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        self.polite
            .insert(host, Instant::now() + Duration::from_millis(self.polite_ms));
    }
}

fn map_reqwest(e: reqwest::Error) -> CrawlError {
    if e.is_timeout() {
        CrawlError::Timeout
    } else if e.is_connect() {
        CrawlError::Io(format!("connect: {e}"))
    } else if e.is_redirect() {
        CrawlError::RedirectLoop
    } else {
        CrawlError::Io(e.to_string())
    }
}

async fn read_capped(resp: reqwest::Response, cap: usize) -> Result<(Vec<u8>, usize), CrawlError> {
    let mut out = Vec::with_capacity(16 * 1024);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_reqwest)?;
        if out.len() + chunk.len() > cap {
            return Err(CrawlError::TooLarge(cap as u64));
        }
        out.extend_from_slice(&chunk);
    }
    let n = out.len();
    Ok((out, n))
}

/// Simple bounded GET used by `bookmark model install`. Not routed through the
/// crawler's SSRF guard because it only ever fetches our own release URLs.
pub async fn download(url: &str, max_bytes: usize) -> Result<Vec<u8>, crate::CrawlError> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| CrawlError::Io(e.to_string()))?;
    let resp = client.get(url).send().await.map_err(map_reqwest)?;
    if !resp.status().is_success() {
        return Err(CrawlError::Status(resp.status().as_u16()));
    }
    let (body, n) = read_capped(resp, max_bytes).await?;
    if n == 0 {
        return Err(CrawlError::Empty);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds() {
        let f = Fetcher::new(4, 1500, 400 * 1024).unwrap();
        assert!(f.client().get("https://example.com").build().is_ok());
    }

    #[tokio::test]
    async fn blocks_localhost_before_network() {
        let f = Fetcher::new(1, 0, 1024).unwrap();
        let u = Url::parse("http://127.0.0.1:1/").unwrap();
        let err = f.fetch(&u).await.unwrap_err();
        assert_eq!(err.kind(), "blocked");
    }
}
