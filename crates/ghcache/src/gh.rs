use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Serialize;
use sqlx::SqliteConnection;
use std::collections::HashMap;
use std::time::Instant;
use tokio::process::Command;

use crate::db;

#[derive(Serialize)]
struct RateSummary {
    ts: String,
    rest_remaining: Option<i64>,
    graphql_remaining: Option<i64>,
}

#[derive(Serialize)]
struct RestCallLine<'a> {
    ts: String,
    endpoint: &'a str,
    status: u16,
    rate_remaining: Option<i64>,
    duration_ms: u64,
}

#[derive(Serialize)]
struct GraphqlCallLine<'a> {
    ts: String,
    endpoint: &'a str,
    status: u16,
    rate_remaining: Option<i64>,
    gql_cost: Option<i64>,
    duration_ms: u64,
}

#[async_trait]
pub trait GitHubClient: Send + Sync {
    async fn call(&self, conn: &mut SqliteConnection, req: &GhRequest<'_>) -> Result<GhResponse>;
    async fn graphql(&self, conn: &mut SqliteConnection, endpoint: &str, query: &str) -> Result<serde_json::Value>;
    async fn throttle_if_needed(&self, conn: &mut SqliteConnection, api_type: &str) -> Result<i64>;
}

pub struct GhClient {
    pub binary:              String,
    pub rate_warn_threshold: i64,
    pub rate_stop_threshold: i64,
    pub silent:              bool,
    pub calls_to_stdout:     bool,
}

pub struct GhRequest<'a> {
    pub endpoint: &'a str,
    pub method:   &'a str,
    pub headers:  Vec<(&'a str, &'a str)>,
    pub body:     Option<&'a str>,
    pub paginate: bool,
}

impl<'a> GhRequest<'a> {
    pub fn get(endpoint: &'a str) -> Self {
        GhRequest { endpoint, method: "GET", headers: vec![], body: None, paginate: false }
    }

    pub fn with_etag(mut self, etag: &'a str) -> Self {
        self.headers.push(("If-None-Match", etag));
        self
    }

    pub fn with_last_modified(mut self, lm: &'a str) -> Self {
        self.headers.push(("If-Modified-Since", lm));
        self
    }

    pub fn paginated(mut self) -> Self {
        self.paginate = true;
        self
    }
}

pub struct GhResponse {
    pub status:      u16,
    pub headers:     HashMap<String, String>,
    pub body:        serde_json::Value,
    #[allow(dead_code)]
    pub duration_ms: u64,
}

impl GhResponse {
    pub fn is_not_modified(&self) -> bool { self.status == 304 }

    pub fn etag(&self) -> Option<&str> {
        self.headers.get("etag").map(|s| s.as_str())
    }

    pub fn last_modified(&self) -> Option<&str> {
        self.headers.get("last-modified").map(|s| s.as_str())
    }

    pub fn poll_interval(&self) -> Option<i64> {
        self.headers.get("x-poll-interval").and_then(|v| v.parse().ok())
    }

    pub fn rate_remaining(&self) -> Option<i64> {
        self.headers.get("x-ratelimit-remaining").and_then(|v| v.parse().ok())
    }

    pub fn rate_reset(&self) -> Option<i64> {
        self.headers.get("x-ratelimit-reset").and_then(|v| v.parse().ok())
    }
}

impl GhClient {
    pub fn new(
        binary: impl Into<String>,
        warn_threshold: i64,
        stop_threshold: i64,
        silent: bool,
        calls_to_stdout: bool,
    ) -> Self {
        GhClient {
            binary: binary.into(),
            rate_warn_threshold: warn_threshold,
            rate_stop_threshold: stop_threshold,
            silent,
            calls_to_stdout,
        }
    }

    async fn emit_rate_log(&self, conn: &mut SqliteConnection) {
        if self.silent { return; }
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        println!("{}", serde_json::to_string(&RateSummary {
            ts,
            rest_remaining:    latest_remaining(conn, "rest").await,
            graphql_remaining: latest_remaining(conn, "graphql").await,
        }).unwrap_or_default());
    }
}

#[async_trait]
impl GitHubClient for GhClient {
    async fn throttle_if_needed(&self, conn: &mut SqliteConnection, api_type: &str) -> Result<i64> {
        let warn = self.rate_warn_threshold;
        let stop = self.rate_stop_threshold;

        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT rate_remaining, COALESCE(rate_reset, 0)
             FROM call_log WHERE api_type = ? AND rate_remaining IS NOT NULL
             ORDER BY id DESC LIMIT 1",
        )
        .bind(api_type)
        .fetch_optional(conn)
        .await?;

        let (remaining, reset_ts) = match row {
            None => return Ok(i64::MAX),
            Some(r) => r,
        };

        if remaining <= stop {
            let now  = chrono::Utc::now().timestamp();
            let wait = (reset_ts - now).max(1);
            tracing::warn!(api_type, remaining, wait_seconds = wait, "rate limit critical -- sleeping until reset");
            tokio::time::sleep(std::time::Duration::from_secs(wait as u64)).await;
        } else if remaining <= warn {
            tracing::warn!(api_type, remaining, "rate limit low -- sleeping 10s");
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }

        Ok(remaining)
    }

    async fn call(&self, conn: &mut SqliteConnection, req: &GhRequest<'_>) -> Result<GhResponse> {
        let mut cmd = Command::new(&self.binary);
        // --include gives us headers (etag, rate limit, poll interval). With
        // --paginate it emits one HTTP block per page; parse_paginated_response
        // takes status and etag from the first block and joins the page bodies.
        cmd.arg("api").arg("--include");

        if req.method != "GET" {
            cmd.arg("-X").arg(req.method);
        }
        if req.paginate {
            cmd.arg("--paginate");
        }
        for (k, v) in &req.headers {
            cmd.arg("-H").arg(format!("{}: {}", k, v));
        }

        cmd.arg(req.endpoint);

        let t = Instant::now();

        let output = if let Some(body) = req.body {
            use tokio::io::AsyncWriteExt;
            let mut child = cmd.stdin(std::process::Stdio::piped()).spawn()
                .context("spawning gh")?;
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(body.as_bytes()).await?;
            drop(stdin);
            child.wait_with_output().await.context("waiting on gh")?
        } else {
            cmd.output().await.context("spawning gh")?
        };

        let duration_ms = t.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let (status, headers, body) = if req.paginate {
            parse_paginated_response(&stdout)
                .with_context(|| format!("parsing paginated response for {}", req.endpoint))?
        } else {
            let (status, headers, body_str) = parse_response(&stdout)?;
            let body = if body_str.trim().is_empty() || status == 304 {
                serde_json::Value::Null
            } else {
                serde_json::from_str(body_str)
                    .with_context(|| format!("parsing JSON body for {}", req.endpoint))?
            };
            (status, headers, body)
        };

        let resp = GhResponse { status, headers, body, duration_ms };

        db::log_call(conn, &db::CallLogEntry {
            endpoint:       req.endpoint,
            api_type:       "rest",
            method:         req.method,
            status_code:    Some(status),
            etag:           resp.etag(),
            last_modified:  resp.last_modified(),
            rate_remaining: resp.rate_remaining(),
            rate_reset:     resp.rate_reset(),
            gql_cost:       None,
            cache_hit:      resp.is_not_modified(),
            duration_ms:    Some(duration_ms as i64),
        })
        .await?;

        if self.calls_to_stdout {
            let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            println!("{}", serde_json::to_string(&RestCallLine {
                ts,
                endpoint: req.endpoint,
                status,
                rate_remaining: resp.rate_remaining(),
                duration_ms,
            }).unwrap_or_default());
        }
        self.emit_rate_log(conn).await;

        // A 304 stores the validators that were sent: the 304 may carry no Etag,
        // or one without the W/ prefix of the weak etag served with the 200.
        let sent = |name: &str| req.headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v);
        let (etag, last_modified) = if resp.is_not_modified() {
            (sent("If-None-Match"), sent("If-Modified-Since"))
        } else {
            (resp.etag(), resp.last_modified())
        };
        db::set_poll_state(
            conn,
            req.endpoint,
            etag,
            last_modified,
            resp.poll_interval(),
            !resp.is_not_modified(),
        )
        .await?;

        Ok(resp)
    }

    async fn graphql(&self, conn: &mut SqliteConnection, endpoint: &str, query: &str) -> Result<serde_json::Value> {
        let instrumented = inject_rate_limit(query);

        let mut cmd = Command::new(&self.binary);
        cmd.arg("api").arg("graphql").arg("--include")
            .arg("-f").arg(format!("query={}", instrumented));

        let t = Instant::now();
        let output = cmd.output().await.context("spawning gh for graphql")?;
        let duration_ms = t.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (status, headers, body_str) = parse_response(&stdout)?;

        if status != 200 {
            bail!("GraphQL request failed with status {}: {}", status, body_str);
        }

        let body: serde_json::Value = serde_json::from_str(body_str)
            .context("parsing GraphQL JSON response")?;

        if let Some(errors) = body.get("errors") {
            bail!("GraphQL errors: {}", errors);
        }

        let gql_cost      = body["data"]["rateLimit"]["cost"].as_i64();
        let gql_remaining = body["data"]["rateLimit"]["remaining"].as_i64();
        let header_remaining = headers.get("x-ratelimit-remaining").and_then(|v| v.parse().ok());
        let header_reset     = headers.get("x-ratelimit-reset").and_then(|v| v.parse().ok());

        db::log_call(conn, &db::CallLogEntry {
            endpoint,
            api_type:       "graphql",
            method:         "POST",
            status_code:    Some(status),
            etag:           None,
            last_modified:  None,
            rate_remaining: gql_remaining.or(header_remaining),
            rate_reset:     header_reset,
            gql_cost,
            cache_hit:      false,
            duration_ms:    Some(duration_ms as i64),
        })
        .await?;

        if self.calls_to_stdout {
            let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            println!("{}", serde_json::to_string(&GraphqlCallLine {
                ts,
                endpoint,
                status,
                rate_remaining: gql_remaining.or(header_remaining),
                gql_cost,
                duration_ms,
            }).unwrap_or_default());
        }
        self.emit_rate_log(conn).await;

        Ok(body["data"].clone())
    }
}

async fn latest_remaining(conn: &mut SqliteConnection, api_type: &str) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT rate_remaining FROM call_log
         WHERE api_type = ? AND rate_remaining IS NOT NULL
         ORDER BY id DESC LIMIT 1",
    )
    .bind(api_type)
    .fetch_optional(conn)
    .await
    .ok()
    .flatten()
}

fn parse_response(raw: &str) -> Result<(u16, HashMap<String, String>, &str)> {
    let last_http = raw.rfind("\nHTTP/").map(|i| i + 1).unwrap_or(0);
    let relevant = &raw[last_http..];

    let sep = relevant.find("\r\n\r\n").or_else(|| relevant.find("\n\n"));
    let (head, body) = match sep {
        Some(i) => {
            let eol_len = if relevant[i..].starts_with("\r\n\r\n") { 4 } else { 2 };
            (&relevant[..i], &relevant[i + eol_len..])
        }
        None => bail!("malformed gh response: no header/body separator"),
    };

    let mut lines = head.lines();
    let status_line = lines.next().context("empty response")?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .context("no status code in response line")?
        .parse()
        .context("parsing status code")?;

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_owned());
        }
    }

    Ok((status, headers, body))
}

fn inject_rate_limit(query: &str) -> String {
    let trimmed = query.trim_end();
    if trimmed.contains("rateLimit") {
        return query.to_owned();
    }
    if let Some(pos) = trimmed.rfind('}') {
        let mut out = trimmed.to_owned();
        out.insert_str(pos, "\n  rateLimit { cost remaining resetAt }\n");
        out
    } else {
        query.to_owned()
    }
}

/// Parse `gh api --include --paginate` output: one HTTP block per page.
/// Status and headers come from the first page (its etag is the one GitHub
/// matches against If-None-Match on the next poll), rate-limit headers from
/// the last page, and array bodies are concatenated.
fn parse_paginated_response(raw: &str) -> Result<(u16, HashMap<String, String>, serde_json::Value)> {
    let mut starts: Vec<usize> = raw.match_indices("\nHTTP/").map(|(i, _)| i + 1).collect();
    if raw.starts_with("HTTP/") { starts.insert(0, 0); }
    if starts.is_empty() { bail!("malformed gh response: no HTTP status line"); }
    starts.push(raw.len());

    let mut first: Option<(u16, HashMap<String, String>)> = None;
    let mut combined: Vec<serde_json::Value> = vec![];
    let mut single: Option<serde_json::Value> = None;
    for w in starts.windows(2) {
        let (status, headers, body_str) = parse_response(&raw[w[0]..w[1]])?;
        let body_str = body_str.trim();
        if !body_str.is_empty() && status != 304 {
            match serde_json::from_str(body_str)
                .with_context(|| format!("parsing page body: {}", &body_str[..body_str.len().min(100)]))?
            {
                serde_json::Value::Array(arr) => combined.extend(arr),
                other => single = Some(other),
            }
        }
        match &mut first {
            None => first = Some((status, headers)),
            Some((_, h)) => for k in ["x-ratelimit-remaining", "x-ratelimit-reset"] {
                if let Some(v) = headers.get(k) { h.insert(k.to_owned(), v.clone()); }
            },
        }
    }
    let (status, headers) = first.expect("at least one block");
    let body = match (status, single) {
        (304, _) => serde_json::Value::Null,
        (_, Some(v)) if combined.is_empty() => v,
        _ => serde_json::Value::Array(combined),
    };
    Ok((status, headers, body))
}

/// Test double. Records all calls and returns queued responses.
#[cfg(test)]
pub(crate) struct MockGhClient {
    pub graphql_calls: std::sync::Mutex<Vec<(String, String)>>,
    graphql_queue:     std::sync::Mutex<std::collections::VecDeque<serde_json::Value>>,
    rest_queue:        std::sync::Mutex<std::collections::VecDeque<(u16, serde_json::Value)>>,
}

#[cfg(test)]
impl MockGhClient {
    pub fn new() -> Self {
        MockGhClient {
            graphql_calls: std::sync::Mutex::new(vec![]),
            graphql_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            rest_queue:    std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    pub fn push_graphql(&self, v: serde_json::Value) {
        self.graphql_queue.lock().unwrap().push_back(v);
    }

    pub fn push_rest(&self, v: serde_json::Value) {
        self.rest_queue.lock().unwrap().push_back((200, v));
    }

    pub fn push_rest_304(&self) {
        self.rest_queue.lock().unwrap().push_back((304, serde_json::Value::Null));
    }

    pub fn graphql_call_count(&self) -> usize {
        self.graphql_calls.lock().unwrap().len()
    }
}

#[cfg(test)]
#[async_trait]
impl GitHubClient for MockGhClient {
    async fn call(&self, _conn: &mut SqliteConnection, _req: &GhRequest<'_>) -> Result<GhResponse> {
        let (status, body) = self.rest_queue.lock().unwrap().pop_front()
            .unwrap_or((200, serde_json::json!([])));
        Ok(GhResponse { status, headers: HashMap::new(), body, duration_ms: 0 })
    }

    async fn graphql(&self, _conn: &mut SqliteConnection, endpoint: &str, query: &str) -> Result<serde_json::Value> {
        self.graphql_calls.lock().unwrap().push((endpoint.to_owned(), query.to_owned()));
        Ok(self.graphql_queue.lock().unwrap().pop_front().unwrap_or(serde_json::json!({})))
    }

    async fn throttle_if_needed(&self, _conn: &mut SqliteConnection, _api_type: &str) -> Result<i64> {
        Ok(i64::MAX)
    }
}

/// Fetch the authenticated GitHub username via `gh api /user`.
pub async fn authenticated_username(binary: &str) -> Result<String> {
    let output = Command::new(binary)
        .args(["api", "/user", "--jq", ".login"])
        .output()
        .await
        .context("fetching authenticated user")?;
    if !output.status.success() {
        bail!("gh api /user failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    let login = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if login.is_empty() {
        bail!("gh api /user returned empty login");
    }
    Ok(login)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_200_response() {
        let raw = "HTTP/2 200\r\ncontent-type: application/json\r\netag: \"abc123\"\r\nX-RateLimit-Remaining: 4998\r\n\r\n{\"foo\":1}";
        let (status, headers, body) = parse_response(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers.get("etag").map(|s| s.as_str()), Some("\"abc123\""));
        assert_eq!(headers.get("x-ratelimit-remaining").map(|s| s.as_str()), Some("4998"));
        assert_eq!(body, "{\"foo\":1}");
    }

    #[test]
    fn parse_304_response() {
        let raw = "HTTP/2 304\r\n\r\n";
        let (status, _, body) = parse_response(raw).unwrap();
        assert_eq!(status, 304);
        assert_eq!(body, "");
    }

    #[test]
    fn parse_lf_only_response() {
        let raw = "HTTP/2 200\ncontent-type: application/json\n\n{\"x\":2}";
        let (status, _, body) = parse_response(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, "{\"x\":2}");
    }

    /// A 304 with no Etag header keeps the etag that was sent, whether or not
    /// poll_state already held it. `gh` is a script printing a bare 304.
    #[tokio::test]
    async fn not_modified_without_etag_keeps_sent_etag() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let fake = tmp.path().join("gh");
        std::fs::write(&fake, "#!/bin/sh\nprintf 'HTTP/2.0 304 Not Modified\\r\\nX-Ratelimit-Remaining: 4000\\r\\n\\r\\n'\nexit 1\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let gh = GhClient::new(fake.to_str().unwrap(), 0, 0, true, false);
        let pool = crate::db::open_in_memory().await.unwrap();
        let mut c = pool.acquire().await.unwrap();
        crate::db::set_poll_state(&mut *c, "/stored", Some("W/\"e\""), None, None, true).await.unwrap();

        let mut got = vec![];
        for ep in ["/stored", "/unstored"] {
            let resp = gh.call(&mut *c, &GhRequest::get(ep).paginated().with_etag("W/\"e\"")).await.unwrap();
            let ps = crate::db::get_poll_state(&mut *c, ep).await.unwrap();
            got.push((ep, resp.status, ps.etag));
        }
        let e = Some("W/\"e\"".to_owned());
        assert_eq!(got, vec![("/stored", 304, e.clone()), ("/unstored", 304, e)]);
    }

    /// Shapes copied from `gh api --include --paginate` output (gh 2.x).
    #[test]
    fn parse_paginated_response_blocks() {
        let pages = "HTTP/2.0 200 OK\r\nEtag: W/\"p1\"\r\nX-Poll-Interval: 60\r\nX-Ratelimit-Remaining: 4990\r\n\r\n[{\"id\":1},{\"id\":2}]\nHTTP/2.0 200 OK\r\nEtag: W/\"p2\"\r\nX-Ratelimit-Remaining: 4989\r\n\r\n[{\"id\":3}]";
        let not_modified = "HTTP/2.0 304 Not Modified\r\nEtag: \"p1\"\r\nX-Ratelimit-Remaining: 4988\r\n\r\n";
        let not_found = "HTTP/2.0 404 Not Found\r\nX-Ratelimit-Remaining: 4987\r\n\r\n{\"message\":\"Not Found\",\"status\":\"404\"}";
        let got: Vec<_> = [pages, not_modified, not_found].iter().map(|raw| {
            let (status, h, body) = parse_paginated_response(raw).unwrap();
            (status, h.get("etag").cloned(), h.get("x-poll-interval").cloned(), h.get("x-ratelimit-remaining").cloned(), body.to_string())
        }).collect();
        let s = |v: &str| Some(v.to_owned());
        assert_eq!(got, vec![
            (200, s("W/\"p1\""), s("60"), s("4989"), r#"[{"id":1},{"id":2},{"id":3}]"#.to_owned()),
            (304, s("\"p1\""), None, s("4988"), "null".to_owned()),
            (404, None, None, s("4987"), r#"{"message":"Not Found","status":"404"}"#.to_owned()),
        ]);
    }

    #[test]
    fn gh_response_helpers() {
        let raw = "HTTP/2 200\r\nETag: \"xyz\"\r\nLast-Modified: Thu, 01 Jan 2026 00:00:00 GMT\r\nX-Poll-Interval: 60\r\nX-RateLimit-Remaining: 100\r\nX-RateLimit-Reset: 1700000000\r\n\r\nnull";
        let (status, headers, body_str) = parse_response(raw).unwrap();
        let resp = GhResponse {
            status,
            headers,
            body: serde_json::from_str(body_str).unwrap(),
            duration_ms: 10,
        };
        assert_eq!(resp.etag(), Some("\"xyz\""));
        assert_eq!(resp.last_modified(), Some("Thu, 01 Jan 2026 00:00:00 GMT"));
        assert_eq!(resp.poll_interval(), Some(60));
        assert_eq!(resp.rate_remaining(), Some(100));
        assert_eq!(resp.rate_reset(), Some(1700000000));
        assert!(!resp.is_not_modified());
    }

    #[test]
    fn parse_picks_last_http_block_for_paginate() {
        let raw = "HTTP/2 200\r\netag: \"first\"\r\n\r\n[{\"id\":1}]\nHTTP/2 200\r\netag: \"second\"\r\n\r\n[{\"id\":2}]";
        let (status, headers, _) = parse_response(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers.get("etag").map(|s| s.as_str()), Some("\"second\""));
    }

    #[test]
    fn inject_rate_limit_adds_selection() {
        let q = "{ repository(owner: \"o\", name: \"n\") { name } }";
        let out = inject_rate_limit(q);
        assert!(out.contains("rateLimit"));
        assert!(out.contains("cost"));
        assert!(out.contains("remaining"));
    }

    #[test]
    fn inject_rate_limit_idempotent() {
        let q = "{ rateLimit { cost remaining resetAt } repository { name } }";
        let out = inject_rate_limit(q);
        assert_eq!(out.matches("rateLimit").count(), 1);
    }

    #[test]
    fn inject_rate_limit_no_closing_brace_is_noop() {
        let q = "no braces here";
        let out = inject_rate_limit(q);
        assert_eq!(out, q);
    }

    #[tokio::test]
    async fn throttle_no_data_returns_max() {
        let pool = crate::db::open_in_memory().await.unwrap();
        let mut c = pool.acquire().await.unwrap();
        let client = GhClient::new("gh", 500, 50, false, false);
        let remaining = client.throttle_if_needed(&mut *c, "rest").await.unwrap();
        assert_eq!(remaining, i64::MAX);
    }

    #[tokio::test]
    async fn throttle_healthy_rate_limit_no_sleep() {
        let pool = crate::db::open_in_memory().await.unwrap();
        let mut c = pool.acquire().await.unwrap();
        crate::db::log_call(&mut *c, &crate::db::CallLogEntry {
            endpoint:       "/repos/o/n/events",
            api_type:       "rest",
            method:         "GET",
            status_code:    Some(200),
            etag:           None,
            last_modified:  None,
            rate_remaining: Some(4800),
            rate_reset:     Some(chrono::Utc::now().timestamp() + 3600),
            gql_cost:       None,
            cache_hit:      false,
            duration_ms:    Some(50),
        })
        .await
        .unwrap();
        let client = GhClient::new("gh", 500, 50, false, false);
        let remaining = client.throttle_if_needed(&mut *c, "rest").await.unwrap();
        assert_eq!(remaining, 4800);
    }

    #[tokio::test]
    async fn throttle_reads_correct_api_type() {
        let pool = crate::db::open_in_memory().await.unwrap();
        let mut c = pool.acquire().await.unwrap();
        crate::db::log_call(&mut *c, &crate::db::CallLogEntry {
            endpoint:       "/repos/o/n/events",
            api_type:       "rest",
            method:         "GET",
            status_code:    Some(200),
            etag:           None,
            last_modified:  None,
            rate_remaining: Some(100),
            rate_reset:     Some(chrono::Utc::now().timestamp() + 3600),
            gql_cost:       None,
            cache_hit:      false,
            duration_ms:    Some(50),
        })
        .await
        .unwrap();
        let client = GhClient::new("gh", 500, 50, false, false);
        let remaining = client.throttle_if_needed(&mut *c, "graphql").await.unwrap();
        assert_eq!(remaining, i64::MAX);
    }
}
