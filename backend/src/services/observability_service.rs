use crate::{config::Config, db::Database};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock, RwLock,
    },
    time::Instant,
};

static PROCESS_STARTED_AT: LazyLock<DateTime<Utc>> = LazyLock::new(Utc::now);
static PROCESS_STARTED_INSTANT: LazyLock<Instant> = LazyLock::new(Instant::now);
static HTTP_METRICS: LazyLock<HttpMetrics> = LazyLock::new(HttpMetrics::default);
static TASKS: LazyLock<RwLock<HashMap<&'static str, TaskMetric>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

const SLOW_REQUEST_MS: u64 = 1_000;

#[derive(Default)]
struct HttpMetrics {
    total_requests: AtomicU64,
    error_requests: AtomicU64,
    slow_requests: AtomicU64,
    total_duration_ms: AtomicU64,
    max_duration_ms: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskMetric {
    pub key: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub interval_secs: Option<u64>,
    pub enabled: bool,
    pub runs: u64,
    pub failures: u64,
    pub consecutive_failures: u64,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub last_duration_ms: Option<u64>,
    pub last_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessOverview {
    pub started_at: DateTime<Utc>,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseOverview {
    pub size: u32,
    pub idle: usize,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpOverview {
    pub total_requests: u64,
    pub error_requests: u64,
    pub slow_requests: u64,
    pub average_duration_ms: u64,
    pub max_duration_ms: u64,
    pub error_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyOverview {
    pub steam_web_api: bool,
    pub steamchina_profile: bool,
    pub steamchina_level: bool,
    pub mysql_map_tiers: bool,
    pub r2_storage: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeConfigOverview {
    pub request_timeout_secs: u64,
    pub max_request_body_bytes: usize,
    pub status_history_retention_secs: u64,
    pub access_log_retention_days: i64,
    pub cors_origin_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservabilityOverview {
    pub generated_at: DateTime<Utc>,
    pub process: ProcessOverview,
    pub database: DatabaseOverview,
    pub http: HttpOverview,
    pub background_tasks: Vec<TaskMetric>,
    pub dependencies: DependencyOverview,
    pub config: RuntimeConfigOverview,
}

pub fn register_task(
    key: &'static str,
    name: &'static str,
    category: &'static str,
    interval_secs: Option<u64>,
    enabled: bool,
) {
    let mut tasks = TASKS.write().expect("task metrics lock poisoned");
    tasks.entry(key).or_insert_with(|| TaskMetric {
        key,
        name,
        category,
        interval_secs,
        enabled,
        runs: 0,
        failures: 0,
        consecutive_failures: 0,
        last_started_at: None,
        last_finished_at: None,
        last_success_at: None,
        last_failure_at: None,
        last_duration_ms: None,
        last_message: None,
    });
}

pub fn record_task_success(key: &'static str, duration_ms: u64, message: impl Into<String>) {
    let mut tasks = TASKS.write().expect("task metrics lock poisoned");
    if let Some(task) = tasks.get_mut(key) {
        let now = Utc::now();
        task.runs += 1;
        task.consecutive_failures = 0;
        task.last_started_at = Some(now - chrono::Duration::milliseconds(duration_ms as i64));
        task.last_finished_at = Some(now);
        task.last_success_at = Some(now);
        task.last_duration_ms = Some(duration_ms);
        let message = message.into();
        task.last_message = (!message.is_empty()).then_some(message);
    }
}

pub fn record_task_failure(key: &'static str, duration_ms: u64, message: impl Into<String>) {
    let mut tasks = TASKS.write().expect("task metrics lock poisoned");
    if let Some(task) = tasks.get_mut(key) {
        let now = Utc::now();
        task.runs += 1;
        task.failures += 1;
        task.consecutive_failures += 1;
        task.last_started_at = Some(now - chrono::Duration::milliseconds(duration_ms as i64));
        task.last_finished_at = Some(now);
        task.last_failure_at = Some(now);
        task.last_duration_ms = Some(duration_ms);
        let message = message.into();
        task.last_message = (!message.is_empty()).then_some(message);
    }
}

pub async fn observe_task<T, E, F>(
    key: &'static str,
    future: F,
    success_message: impl FnOnce(&T) -> String,
) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let started = Instant::now();
    let result = future.await;
    let duration_ms = started.elapsed().as_millis() as u64;
    match &result {
        Ok(value) => record_task_success(key, duration_ms, success_message(value)),
        Err(error) => record_task_failure(key, duration_ms, error.to_string()),
    }
    result
}

pub fn record_http_request(status: u16, duration_ms: u64) {
    HTTP_METRICS.total_requests.fetch_add(1, Ordering::Relaxed);
    HTTP_METRICS
        .total_duration_ms
        .fetch_add(duration_ms, Ordering::Relaxed);
    if status >= 500 {
        HTTP_METRICS.error_requests.fetch_add(1, Ordering::Relaxed);
    }
    if duration_ms >= SLOW_REQUEST_MS {
        HTTP_METRICS.slow_requests.fetch_add(1, Ordering::Relaxed);
    }
    update_max(&HTTP_METRICS.max_duration_ms, duration_ms);
}

fn update_max(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

pub fn overview(db: &Database, config: &Config) -> ObservabilityOverview {
    let total_requests = HTTP_METRICS.total_requests.load(Ordering::Relaxed);
    let error_requests = HTTP_METRICS.error_requests.load(Ordering::Relaxed);
    let total_duration_ms = HTTP_METRICS.total_duration_ms.load(Ordering::Relaxed);
    let error_rate = if total_requests == 0 {
        0.0
    } else {
        (error_requests as f64 / total_requests as f64) * 100.0
    };

    let mut background_tasks: Vec<TaskMetric> = TASKS
        .read()
        .expect("task metrics lock poisoned")
        .values()
        .cloned()
        .collect();
    background_tasks.sort_by(|a, b| a.category.cmp(b.category).then(a.name.cmp(b.name)));

    ObservabilityOverview {
        generated_at: Utc::now(),
        process: ProcessOverview {
            started_at: *PROCESS_STARTED_AT,
            uptime_seconds: PROCESS_STARTED_INSTANT.elapsed().as_secs(),
        },
        database: DatabaseOverview {
            size: db.pool.size(),
            idle: db.pool.num_idle(),
            max_connections: config.db_max_connections,
            min_connections: config.db_min_connections,
            acquire_timeout_secs: config.db_acquire_timeout_secs,
            idle_timeout_secs: config.db_idle_timeout_secs,
        },
        http: HttpOverview {
            total_requests,
            error_requests,
            slow_requests: HTTP_METRICS.slow_requests.load(Ordering::Relaxed),
            average_duration_ms: if total_requests == 0 {
                0
            } else {
                total_duration_ms / total_requests
            },
            max_duration_ms: HTTP_METRICS.max_duration_ms.load(Ordering::Relaxed),
            error_rate,
        },
        background_tasks,
        dependencies: DependencyOverview {
            steam_web_api: config.steam_web_key.is_some(),
            steamchina_profile: config.steamchina_profile_key.is_some(),
            steamchina_level: config.steamchina_level_key.is_some(),
            mysql_map_tiers: config.mysql_database_url.is_some(),
            r2_storage: config.r2_endpoint.is_some()
                && config.r2_bucket.is_some()
                && config.r2_access_key_id.is_some()
                && config.r2_secret_access_key.is_some(),
        },
        config: RuntimeConfigOverview {
            request_timeout_secs: config.request_timeout_secs,
            max_request_body_bytes: config.max_request_body_bytes,
            status_history_retention_secs: config.status_history_retention_secs,
            access_log_retention_days: config.access_log_retention_days,
            cors_origin_configured: config.cors_origin.is_some(),
        },
    }
}
