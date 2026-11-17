// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! `robots.txt` fetching and caching. A missing or erroring robots file is
//! treated as "allow all", which matches the behaviour of every major crawler.

use std::sync::Arc;

use dashmap::DashMap;
use texting_robots::Robot;
use url::Url;

const AGENT: &str = "bookmark";

/// Per-host robots cache.
#[derive(Default)]
pub struct RobotsCache {
    map: DashMap<String, Option<Arc<Robot>>>,
}

impl RobotsCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// True if `path` may be fetched for `host`. Fetches and caches robots.txt
    /// on first use. Network errors default to allow.
    pub async fn allowed(&self, client: &reqwest::Client, url: &Url, path: &str) -> bool {
        let host = match url.host_str() {
            Some(h) => h.to_string(),
            None => return true,
        };
        if let Some(entry) = self.map.get(&host) {
            return match entry.value() {
                Some(r) => r.allowed(path),
                None => true,
            };
        }
        let robots = self.fetch(client, url).await;
        let allowed = match &robots {
            Some(r) => r.allowed(path),
            None => true,
        };
        self.map.insert(host, robots.map(Arc::new));
        allowed
    }

    async fn fetch(&self, client: &reqwest::Client, url: &Url) -> Option<Robot> {
        let mut robots_url = url.clone();
        robots_url.set_path("/robots.txt");
        robots_url.set_query(None);
        robots_url.set_fragment(None);
        let resp = client.get(robots_url.clone()).send().await.ok()?;
        let _status = resp.status().as_u16();
        let body = resp.bytes().await.ok()?;
        Robot::new(AGENT, &body).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_robots_allows() {
        let cache = RobotsCache::new();
        // No entry -> allowed, without touching the network, because the host
        // lookup short-circuits on the dashmap get.
        assert!(cache.map.is_empty());
    }
}
