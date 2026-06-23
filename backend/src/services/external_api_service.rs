use chrono::{DateTime, Utc};
use reqwest::{Response, StatusCode, header::HeaderMap};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize)]
pub struct ExternalApiMetric {
    pub key: &'static str,
    pub name: &'static str,
    pub requests: u64,
    pub failures: u64,
    pub rate_limited: u64,
    pub consecutive_failures: u64,
    pub last_request_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub last_status: Option<u16>,
    pub last_error: Option<String>,
    pub cooldown_until: Option<DateTime<Utc>>,
}

struct ExternalApiState {
    metric: ExternalApiMetric,
    cooldown_until_instant: Option<Instant>,
}

static EXTERNAL_APIS: LazyLock<RwLock<HashMap<&'static str, ExternalApiState>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static EXTERNAL_API_LOCKS: LazyLock<RwLock<HashMap<&'static str, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn ensure_metric(key: &'static str, name: &'static str) {
    let mut apis = EXTERNAL_APIS.write().expect("external api metrics lock poisoned");
    apis.entry(key).or_insert_with(|| ExternalApiState {
        metric: ExternalApiMetric {
            key,
            name,
            requests: 0,
            failures: 0,
            rate_limited: 0,
            consecutive_failures: 0,
            last_request_at: None,
            last_success_at: None,
            last_failure_at: None,
            last_status: None,
            last_error: None,
            cooldown_until: None,
        },
        cooldown_until_instant: None,
    });
}

fn api_lock(key: &'static str) -> Arc<tokio::sync::Mutex<()>> {
    {
        let locks = EXTERNAL_API_LOCKS.read().expect("external api locks lock poisoned");
        if let Some(lock) = locks.get(key) {
            return lock.clone();
        }
    }
    let mut locks = EXTERNAL_API_LOCKS.write().expect("external api locks lock poisoned");
    locks
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

fn cooldown_remaining(key: &'static str) -> Option<Duration> {
    let mut apis = EXTERNAL_APIS.write().ok()?;
    let state = apis.get_mut(key)?;
    clear_expired_cooldown(state);
    match state.cooldown_until_instant {
        Some(until) if until > Instant::now() => Some(until.duration_since(Instant::now())),
        _ => None,
    }
}

fn clear_expired_cooldown(state: &mut ExternalApiState) {
    if state
        .cooldown_until_instant
        .is_some_and(|until| until <= Instant::now())
    {
        state.cooldown_until_instant = None;
        state.metric.cooldown_until = None;
    }
}

fn retry_after_duration(headers: &HeaderMap) -> Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(60))
}

fn record_request(key: &'static str, name: &'static str) {
    ensure_metric(key, name);
    if let Ok(mut apis) = EXTERNAL_APIS.write() {
        if let Some(state) = apis.get_mut(key) {
            state.metric.requests += 1;
            state.metric.last_request_at = Some(Utc::now());
        }
    }
}

fn record_success(key: &'static str, status: StatusCode) {
    if let Ok(mut apis) = EXTERNAL_APIS.write() {
        if let Some(state) = apis.get_mut(key) {
            state.metric.consecutive_failures = 0;
            state.metric.last_success_at = Some(Utc::now());
            state.metric.last_status = Some(status.as_u16());
            state.metric.last_error = None;
            state.metric.cooldown_until = None;
            state.cooldown_until_instant = None;
        }
    }
}

fn record_failure(key: &'static str, status: Option<StatusCode>, error: impl Into<String>) {
    if let Ok(mut apis) = EXTERNAL_APIS.write() {
        if let Some(state) = apis.get_mut(key) {
            state.metric.failures += 1;
            state.metric.consecutive_failures += 1;
            state.metric.last_failure_at = Some(Utc::now());
            state.metric.last_status = status.map(|s| s.as_u16());
            state.metric.last_error = Some(error.into());
        }
    }
}

fn record_rate_limit(key: &'static str, cooldown: Duration) {
    if let Ok(mut apis) = EXTERNAL_APIS.write() {
        if let Some(state) = apis.get_mut(key) {
            state.metric.rate_limited += 1;
            state.metric.cooldown_until =
                Some(Utc::now() + chrono::Duration::seconds(cooldown.as_secs() as i64));
            state.cooldown_until_instant = Some(Instant::now() + cooldown);
        }
    }
}

pub async fn get_json<T>(
    key: &'static str,
    name: &'static str,
    url: &str,
    timeout: Duration,
) -> anyhow::Result<T>
where
    T: DeserializeOwned,
{
    ensure_metric(key, name);
    if let Some(remaining) = cooldown_remaining(key) {
        anyhow::bail!(
            "{} 正在限流冷却，请 {} 秒后重试",
            name,
            remaining.as_secs().max(1)
        );
    }

    let lock = api_lock(key);
    let _guard = lock.lock().await;

    if let Some(remaining) = cooldown_remaining(key) {
        anyhow::bail!(
            "{} 正在限流冷却，请 {} 秒后重试",
            name,
            remaining.as_secs().max(1)
        );
    }

    record_request(key, name);
    let response = send_get(url, timeout).await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            record_failure(key, None, error.to_string());
            return Err(error);
        }
    };

    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let cooldown = retry_after_duration(response.headers());
        record_rate_limit(key, cooldown);
        record_failure(
            key,
            Some(status),
            format!("HTTP 429 Too Many Requests，已暂停请求 {} 秒", cooldown.as_secs()),
        );
        anyhow::bail!(
            "{} 返回 429 Too Many Requests，已暂停请求 {} 秒",
            name,
            cooldown.as_secs()
        );
    }
    if !status.is_success() {
        record_failure(key, Some(status), format!("HTTP {status}"));
        anyhow::bail!("{} 返回 {status}", name);
    }

    let parsed = response.json::<T>().await.map_err(|error| {
        record_failure(key, Some(status), error.to_string());
        error
    })?;
    record_success(key, status);
    Ok(parsed)
}

async fn send_get(url: &str, timeout: Duration) -> anyhow::Result<Response> {
    let client = crate::http_client::http_client();
    let response = tokio::time::timeout(timeout, client.get(url).send())
        .await
        .map_err(|_| anyhow::anyhow!("外部 API 请求超时 ({}s)", timeout.as_secs()))?
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(response)
}

pub fn metrics() -> Vec<ExternalApiMetric> {
    let mut apis = EXTERNAL_APIS
        .write()
        .expect("external api metrics lock poisoned");
    let mut items: Vec<ExternalApiMetric> = apis
        .values_mut()
        .map(|state| {
            clear_expired_cooldown(state);
            state.metric.clone()
        })
        .collect();
    items.sort_by(|a, b| a.name.cmp(b.name));
    items
}

pub fn metric(key: &'static str) -> Option<ExternalApiMetric> {
    EXTERNAL_APIS
        .write()
        .ok()
        .and_then(|mut apis| {
            apis.get_mut(key).map(|state| {
                clear_expired_cooldown(state);
                state.metric.clone()
            })
        })
}
