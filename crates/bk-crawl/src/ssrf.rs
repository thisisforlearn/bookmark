// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! SSRF guard. We resolve the hostname ourselves, reject any private/loopback/
//! link-local/CGNAT/multicast/unspecified address, and pin the resolved IP so a
//! DNS-rebinding attack can't swap it after the check.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use url::Url;

use bk_core::{Error, Result};

/// Reject a URL whose host resolves to a non-public address.
/// Returns the resolved public IPs so the caller can pin them.
pub fn check(url: &Url) -> Result<Vec<IpAddr>> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::Crawl("url has no host".into()))?;
    // Already-an-IP host: check directly, no DNS.
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if !is_public(ip) {
            return Err(Error::Crawl("blocked: private address".into()));
        }
        return Ok(vec![ip]);
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<IpAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| Error::Crawl(format!("dns: {e}")))?
        .map(|s| s.ip())
        .collect();
    if addrs.is_empty() {
        return Err(Error::Crawl("dns: no addresses".into()));
    }
    for ip in &addrs {
        if !is_public(*ip) {
            return Err(Error::Crawl(
                "blocked: resolves to a private address".into(),
            ));
        }
    }
    Ok(addrs)
}

/// True if the address is a public unicast address we are willing to fetch.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        // CGNAT 100.64.0.0/10
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        // 0.0.0.0/8
        || o[0] == 0
        // 198.18.0.0/15 benchmarking
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        // 192.0.0.0/24 IETF protocol assignments
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        // 240.0.0.0/4 reserved
        || o[0] >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    let seg = ip.segments();
    // Unique local fc00::/7
    if seg[0] & 0xfe00 == 0xfc00 {
        return false;
    }
    // Link-local fe80::/10
    if seg[0] & 0xffc0 == 0xfe80 {
        return false;
    }
    // IPv4-mapped ::ffff:0:0/96 — re-check the embedded v4.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_ranges() {
        for s in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "240.0.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
        ] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(!is_public(ip), "{s} should be blocked");
        }
    }

    #[test]
    fn allows_public() {
        for s in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(is_public(ip), "{s} should be allowed");
        }
    }

    #[test]
    fn blocks_localhost_url() {
        let u = Url::parse("http://localhost/").unwrap();
        assert!(check(&u).is_err());
        let u = Url::parse("http://169.254.169.254/latest/meta-data").unwrap();
        assert!(check(&u).is_err());
    }
}
