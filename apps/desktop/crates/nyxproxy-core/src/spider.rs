//! Spider — scope-aware BFS crawler.
//!
//! Given a starting URL and an in-scope predicate, the spider repeatedly
//! issues GET requests with `reqwest`, parses HTML responses for embedded
//! links / forms / inline `src`+`href` attributes, queues every newly seen
//! in-scope URL, and emits each visited node as a [`SpiderHit`] over a
//! stream. The crawler obeys `robots.txt` by default and limits its
//! breadth + depth so it can be run safely in interactive mode.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpiderConfig {
    pub seed_url: String,
    pub scope_hosts: Vec<String>,
    /// Maximum depth from the seed (seed is depth 0).
    #[serde(default = "default_depth")]
    pub max_depth: u32,
    /// Total URLs to visit. Caps memory + outbound bandwidth.
    #[serde(default = "default_max_urls")]
    pub max_urls: usize,
    /// Concurrency cap.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_true")]
    pub follow_robots: bool,
    #[serde(default)]
    pub insecure: bool,
}

fn default_depth() -> u32 {
    3
}

fn default_max_urls() -> usize {
    200
}

fn default_concurrency() -> usize {
    4
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpiderHit {
    pub url: String,
    pub depth: u32,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub bytes: Option<usize>,
    pub elapsed_ms: u64,
    pub linked_count: usize,
    pub error: Option<String>,
}

/// Run the spider to completion, returning every visited node. The function
/// is `async` so the caller can `tokio::spawn` it and read partial progress
/// from the optional `progress` channel.
pub async fn crawl(
    cfg: SpiderConfig,
    progress: Option<tokio::sync::mpsc::Sender<SpiderHit>>,
) -> Vec<SpiderHit> {
    let client_builder = Client::builder()
        .user_agent("NyxProxy-Spider/0.1")
        .redirect(reqwest::redirect::Policy::limited(5));
    let client_builder = if cfg.insecure {
        client_builder.danger_accept_invalid_certs(true)
    } else {
        client_builder
    };
    let client = client_builder
        .build()
        .expect("reqwest client should build");

    let seen: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let queue: Arc<Mutex<VecDeque<(String, u32)>>> = Arc::new(Mutex::new(VecDeque::new()));
    let out: Arc<Mutex<Vec<SpiderHit>>> = Arc::new(Mutex::new(Vec::new()));

    let seed_url = normalize(&cfg.seed_url);
    seen.lock().insert(seed_url.clone());
    queue.lock().push_back((seed_url, 0));

    let semaphore = Arc::new(tokio::sync::Semaphore::new(cfg.concurrency.max(1)));
    let scope_hosts = cfg.scope_hosts.clone();
    let robots_cache: Arc<Mutex<std::collections::HashMap<String, Vec<String>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    let mut handles = Vec::new();
    loop {
        let total = out.lock().len();
        if total >= cfg.max_urls {
            break;
        }
        let next = queue.lock().pop_front();
        let (url, depth) = match next {
            Some(item) => item,
            None => {
                // Wait for in-flight tasks to push more.
                if handles.is_empty() {
                    break;
                }
                // Drain at least one outstanding task before polling again.
                if let Some(h) = handles.pop() {
                    let _: Result<(), tokio::task::JoinError> = h.await;
                }
                continue;
            }
        };
        let permit = semaphore.clone().acquire_owned().await.expect("semaphore");
        let client = client.clone();
        let seen = seen.clone();
        let queue = queue.clone();
        let out = out.clone();
        let progress = progress.clone();
        let scope_hosts = scope_hosts.clone();
        let robots_cache = robots_cache.clone();
        let follow_robots = cfg.follow_robots;
        let max_depth = cfg.max_depth;
        let max_urls = cfg.max_urls;
        handles.push(tokio::spawn(async move {
            let _permit = permit; // keep alive for duration
            let mut hit = SpiderHit {
                url: url.clone(),
                depth,
                status: None,
                content_type: None,
                bytes: None,
                elapsed_ms: 0,
                linked_count: 0,
                error: None,
            };
            if follow_robots {
                if let Some(allowed) =
                    robots_allows(&client, &robots_cache, &url, "NyxProxy-Spider").await
                {
                    if !allowed {
                        hit.error = Some("disallowed by robots.txt".into());
                        if let Some(tx) = progress.as_ref() {
                            let _ = tx.send(hit.clone()).await;
                        }
                        out.lock().push(hit);
                        return;
                    }
                }
            }
            let started = Instant::now();
            let resp = client.get(&url).send().await;
            hit.elapsed_ms = started.elapsed().as_millis() as u64;
            match resp {
                Ok(r) => {
                    hit.status = Some(r.status().as_u16());
                    hit.content_type = r
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let is_html = hit
                        .content_type
                        .as_ref()
                        .map(|c| c.to_ascii_lowercase().contains("html"))
                        .unwrap_or(false);
                    match r.bytes().await {
                        Ok(bytes) => {
                            hit.bytes = Some(bytes.len());
                            if is_html {
                                let body = String::from_utf8_lossy(&bytes);
                                let links = extract_links(&url, &body);
                                hit.linked_count = links.len();
                                if depth + 1 <= max_depth {
                                    let mut q = queue.lock();
                                    let mut s = seen.lock();
                                    let visited_so_far = out.lock().len();
                                    for link in links {
                                        if s.len() + visited_so_far + q.len() >= max_urls {
                                            break;
                                        }
                                        if !in_scope(&link, &scope_hosts) {
                                            continue;
                                        }
                                        if !s.insert(link.clone()) {
                                            continue;
                                        }
                                        q.push_back((link, depth + 1));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            hit.error = Some(format!("body: {e}"));
                        }
                    }
                }
                Err(e) => {
                    hit.error = Some(format!("send: {e}"));
                }
            }
            if let Some(tx) = progress.as_ref() {
                let _ = tx.send(hit.clone()).await;
            }
            out.lock().push(hit);
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    Arc::try_unwrap(out).unwrap_or_else(|a| Mutex::new(a.lock().clone())).into_inner()
}

fn normalize(url: &str) -> String {
    if let Ok(parsed) = Url::parse(url) {
        parsed.to_string()
    } else {
        url.to_string()
    }
}

fn in_scope(url: &str, hosts: &[String]) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = match parsed.host_str() {
        Some(h) => h,
        None => return false,
    };
    if hosts.is_empty() {
        return true;
    }
    hosts
        .iter()
        .any(|h| host == h || host.ends_with(&format!(".{h}")))
}

/// Extract HTML links — `href=`, `src=`, `action=` — and resolve them against
/// the base URL. Returns a deduplicated, ordered vector.
pub fn extract_links(base: &str, html: &str) -> Vec<String> {
    let base_url = match Url::parse(base) {
        Ok(u) => u,
        Err(_) => return Vec::new(),
    };
    let re = Regex::new(r#"(?i)(?:href|src|action)\s*=\s*["']([^"']+)["']"#).unwrap();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for cap in re.captures_iter(html) {
        let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if raw.starts_with("javascript:") || raw.starts_with("mailto:") || raw.starts_with("#") {
            continue;
        }
        if let Ok(joined) = base_url.join(raw) {
            // Drop fragment.
            let mut j = joined.clone();
            j.set_fragment(None);
            let s = j.to_string();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
    }
    out
}

async fn robots_allows(
    client: &Client,
    cache: &Arc<Mutex<std::collections::HashMap<String, Vec<String>>>>,
    url: &str,
    user_agent: &str,
) -> Option<bool> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_string();
    let robots_url = format!("{}://{}/robots.txt", parsed.scheme(), host);
    let cached = {
        let c = cache.lock();
        c.get(&host).cloned()
    };
    let disallow_patterns = if let Some(p) = cached {
        p
    } else {
        let body = match client.get(&robots_url).send().await {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            _ => String::new(),
        };
        let patterns = parse_robots_disallow(&body, user_agent);
        cache.lock().insert(host.clone(), patterns.clone());
        patterns
    };
    let path = parsed.path();
    let allowed = !disallow_patterns
        .iter()
        .any(|p| !p.is_empty() && path.starts_with(p.as_str()));
    Some(allowed)
}

fn parse_robots_disallow(body: &str, user_agent: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut active = false;
    let ua_lower = user_agent.to_ascii_lowercase();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(':') {
            Some(kv) => (kv.0.trim().to_ascii_lowercase(), kv.1.trim().to_string()),
            None => continue,
        };
        match key.as_str() {
            "user-agent" => {
                let v = value.to_ascii_lowercase();
                active = v == "*" || ua_lower.contains(&v);
            }
            "disallow" if active => {
                out.push(value);
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_links_resolves_relative() {
        let html = r#"<a href="/foo">x</a><a href="https://other.test/x">y</a><img src="img/a.png">"#;
        let links = extract_links("https://example.com/path/", html);
        assert!(links.iter().any(|l| l == "https://example.com/foo"));
        assert!(links.iter().any(|l| l == "https://other.test/x"));
        assert!(links.iter().any(|l| l == "https://example.com/path/img/a.png"));
    }

    #[test]
    fn scope_filter_matches_subdomains() {
        assert!(in_scope("https://api.example.com/x", &["example.com".into()]));
        assert!(in_scope("https://example.com/", &["example.com".into()]));
        assert!(!in_scope("https://attacker.test/", &["example.com".into()]));
    }

    #[test]
    fn parse_robots_disallow_collects_per_agent() {
        let body = "User-agent: *\nDisallow: /private\nDisallow: /admin\n";
        let out = parse_robots_disallow(body, "NyxProxy-Spider");
        assert_eq!(out, vec!["/private", "/admin"]);
    }
}
