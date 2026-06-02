use crate::{db::Database, http_client::http_client};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

const DEFAULT_SOURCE_URLS: [&str; 2] = [
    "https://files.femboykz.com/fastdl/csgo/maps/",
    "https://download.axekz.com/csgo/maps/",
];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MapSyncConfig {
    pub enabled: bool,
    pub auto_update: bool,
    pub source_urls: Vec<String>,
    pub map_pool_url: String,
    pub check_interval_secs: i32,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MapSyncAgent {
    pub id: Uuid,
    pub name: String,
    pub target_type: String,
    pub token: String,
    pub enabled: bool,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub last_inventory_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MapSyncTask {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub agent_name: Option<String>,
    pub target_type: Option<String>,
    pub map_name: String,
    pub file_name: String,
    pub source_url: String,
    pub source_size_bytes: Option<i64>,
    pub source_modified_at: Option<DateTime<Utc>>,
    pub status: String,
    pub reason: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MapSyncOverview {
    pub config: MapSyncConfig,
    pub agents: Vec<MapSyncAgent>,
    pub recent_tasks: Vec<MapSyncTask>,
    pub map_pool_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMapSyncConfigInput {
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub auto_update: bool,
    pub source_urls: Vec<String>,
    pub map_pool_url: String,
    #[serde(default = "default_interval")]
    pub check_interval_secs: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMapSyncAgentInput {
    pub name: String,
    pub target_type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentInventoryInput {
    pub maps: Vec<AgentMapInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentMapInput {
    pub file_name: String,
    pub size_bytes: i64,
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentTaskReportInput {
    pub status: String,
    pub error: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MapSyncCheckResult {
    pub maps_found: usize,
    pub agents_checked: usize,
    pub tasks_created: usize,
    pub skipped_files: usize,
    pub unavailable_files: usize,
}

#[derive(Debug, Clone)]
struct RemoteMapSet {
    raw: Option<RemoteFile>,
    compressed: Option<RemoteFile>,
}

#[derive(Debug, Clone)]
struct RemoteFile {
    file_name: String,
    url: String,
    size: Option<i64>,
    last_modified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
struct KzMapItem {
    name: String,
    difficulty: Option<i32>,
}

fn default_true() -> bool {
    true
}

fn default_interval() -> i32 {
    3600
}

pub async fn overview(db: &Database) -> anyhow::Result<MapSyncOverview> {
    Ok(MapSyncOverview {
        config: get_config(db).await?,
        agents: list_agents(db).await?,
        recent_tasks: list_recent_tasks(db).await?,
        map_pool_names: cached_remote_map_names(db).await?,
    })
}

pub async fn get_config(db: &Database) -> anyhow::Result<MapSyncConfig> {
    ensure_config_row(db).await?;
    sqlx::query_as::<_, MapSyncConfig>(
        r#"SELECT enabled, auto_update, source_urls, map_pool_url, check_interval_secs,
                  last_checked_at, last_status, last_error, updated_at
           FROM map_sync_config
           WHERE id = true"#,
    )
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn update_config(
    db: &Database,
    input: UpdateMapSyncConfigInput,
) -> anyhow::Result<MapSyncConfig> {
    let source_urls = normalize_urls(input.source_urls);
    let map_pool_url = input.map_pool_url.trim();
    anyhow::ensure!(!source_urls.is_empty(), "至少需要配置一个地图下载源");
    anyhow::ensure!(!map_pool_url.is_empty(), "地图池 API URL 不能为空");

    sqlx::query_as::<_, MapSyncConfig>(
        r#"INSERT INTO map_sync_config (
             id, enabled, auto_update, source_urls, map_pool_url, check_interval_secs, updated_at
           )
           VALUES (true, $1, $2, $3, $4, $5, now())
           ON CONFLICT (id) DO UPDATE SET
             enabled = EXCLUDED.enabled,
             auto_update = EXCLUDED.auto_update,
             source_urls = EXCLUDED.source_urls,
             map_pool_url = EXCLUDED.map_pool_url,
             check_interval_secs = EXCLUDED.check_interval_secs,
             updated_at = now()
           RETURNING enabled, auto_update, source_urls, map_pool_url, check_interval_secs,
                     last_checked_at, last_status, last_error, updated_at"#,
    )
    .bind(input.enabled)
    .bind(input.auto_update)
    .bind(&source_urls)
    .bind(map_pool_url)
    .bind(input.check_interval_secs.clamp(60, 86_400))
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn create_agent(
    db: &Database,
    input: CreateMapSyncAgentInput,
) -> anyhow::Result<MapSyncAgent> {
    let name = input.name.trim();
    let target_type = input.target_type.trim();
    anyhow::ensure!(!name.is_empty(), "代理名称不能为空");
    anyhow::ensure!(
        matches!(target_type, "game" | "download"),
        "代理类型必须是 game 或 download"
    );

    sqlx::query_as::<_, MapSyncAgent>(
        r#"INSERT INTO map_sync_agents (name, target_type, enabled)
           VALUES ($1, $2, $3)
           RETURNING id, name, target_type, token, enabled, last_seen_at, last_inventory_at, created_at"#,
    )
    .bind(name)
    .bind(target_type)
    .bind(input.enabled)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn delete_agent(db: &Database, id: Uuid) -> anyhow::Result<()> {
    let result = sqlx::query("DELETE FROM map_sync_agents WHERE id = $1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    anyhow::ensure!(result.rows_affected() > 0, "代理不存在");
    Ok(())
}

pub async fn reset_agent_token(db: &Database, id: Uuid) -> anyhow::Result<MapSyncAgent> {
    sqlx::query_as::<_, MapSyncAgent>(
        r#"UPDATE map_sync_agents
           SET token = md5(random()::TEXT || clock_timestamp()::TEXT)
           WHERE id = $1
           RETURNING id, name, target_type, token, enabled, last_seen_at, last_inventory_at, created_at"#,
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn check_and_enqueue_all(db: &Database) -> anyhow::Result<MapSyncCheckResult> {
    let config = get_config(db).await?;
    let result = check_and_enqueue(db, &config, None, false).await;
    persist_check_result(db, &result).await?;
    result
}

pub async fn enqueue_single_map(
    db: &Database,
    map_name: &str,
) -> anyhow::Result<MapSyncCheckResult> {
    let normalized = normalize_map_name(map_name)
        .ok_or_else(|| anyhow::anyhow!("地图名不能为空，请填写例如 kz_example 或 kz_example.bsp"))?;
    let config = get_config(db).await?;
    let result = check_and_enqueue(db, &config, Some(normalized), true).await;
    persist_check_result(db, &result).await?;
    result
}

pub fn start_map_sync_loop(db: Database, scan_interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(scan_interval_secs.max(30)));
        loop {
            interval.tick().await;
            if let Err(error) = enqueue_due_maps(&db).await {
                tracing::warn!(%error, "地图定时检测失败");
            }
        }
    });
}

async fn enqueue_due_maps(db: &Database) -> anyhow::Result<()> {
    let config = get_config(db).await?;
    if !config.enabled || !config.auto_update {
        return Ok(());
    }
    let interval = ChronoDuration::seconds(config.check_interval_secs.max(60) as i64);
    if let Some(last_checked_at) = config.last_checked_at {
        if Utc::now() - last_checked_at < interval {
            return Ok(());
        }
    }
    let result = check_and_enqueue(db, &config, None, false).await;
    persist_check_result(db, &result).await?;
    match result {
        Ok(summary) => tracing::info!(
            tasks_created = summary.tasks_created,
            skipped = summary.skipped_files,
            "地图定时检测完成"
        ),
        Err(error) => tracing::warn!(%error, "地图定时检测失败"),
    }
    Ok(())
}

async fn check_and_enqueue(
    db: &Database,
    config: &MapSyncConfig,
    selected_map: Option<String>,
    force: bool,
) -> anyhow::Result<MapSyncCheckResult> {
    let agents = list_enabled_agents(db).await?;
    anyhow::ensure!(!agents.is_empty(), "请先创建至少一个地图同步代理");

    let remote_maps = load_remote_maps(&config.source_urls).await?;
    let desired_map_names = load_map_pool_names(&config.map_pool_url).await?;
    persist_remote_map_cache(db, &desired_map_names, &remote_maps).await?;
    let mut map_names = if let Some(map_name) = selected_map {
        vec![map_name]
    } else {
        desired_map_names
    };
    map_names.sort();
    let maps_found = map_names.len();

    let mut tasks_created = 0usize;
    let mut skipped_files = 0usize;
    let mut unavailable_files = 0usize;

    for agent in &agents {
        for map_name in &map_names {
            let Some(remote) = remote_maps.get(map_name) else {
                unavailable_files += 1;
                continue;
            };
            unavailable_files += missing_required_source_count(&agent.target_type, remote);
            let sources = select_sources(&agent.target_type, remote);
            if sources.is_empty() {
                continue;
            }
            for source in sources {
                let source = ensure_remote_meta(&source).await?;
                let reason = if force {
                    Some("手动指定更新".to_string())
                } else {
                    stale_reason(db, agent.id, &source).await?
                };
                if let Some(reason) = reason {
                    if !has_active_task(db, agent.id, &source.file_name).await? {
                        create_task(db, agent.id, map_name, &source, &reason).await?;
                        tasks_created += 1;
                    } else {
                        skipped_files += 1;
                    }
                } else {
                    skipped_files += 1;
                }
            }
        }
    }

    Ok(MapSyncCheckResult {
        maps_found,
        agents_checked: agents.len(),
        tasks_created,
        skipped_files,
        unavailable_files,
    })
}

pub async fn agent_by_token(db: &Database, token: &str) -> anyhow::Result<MapSyncAgent> {
    let token = token.trim();
    anyhow::ensure!(!token.is_empty(), "缺少地图同步代理 token");
    let agent = sqlx::query_as::<_, MapSyncAgent>(
        r#"UPDATE map_sync_agents
           SET last_seen_at = now()
           WHERE token = $1 AND enabled = true
           RETURNING id, name, target_type, token, enabled, last_seen_at, last_inventory_at, created_at"#,
    )
    .bind(token)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("地图同步代理不存在或已禁用"))?;
    Ok(agent)
}

pub async fn report_inventory(
    db: &Database,
    agent: &MapSyncAgent,
    input: AgentInventoryInput,
) -> anyhow::Result<usize> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM map_sync_agent_maps WHERE agent_id = $1")
        .bind(agent.id)
        .execute(&mut *tx)
        .await?;

    let mut inserted = 0usize;
    for chunk in input.maps.chunks(500) {
        let mut file_names = Vec::new();
        let mut map_names = Vec::new();
        let mut sizes = Vec::new();
        let mut modified = Vec::new();
        for item in chunk {
            if item.size_bytes < 0 {
                continue;
            }
            let Some(file_name) = safe_map_file_name(&item.file_name) else {
                continue;
            };
            let Some(map_name) = normalize_map_name(&file_name) else {
                continue;
            };
            file_names.push(file_name);
            map_names.push(map_name);
            sizes.push(item.size_bytes);
            modified.push(item.modified_at);
        }
        if file_names.is_empty() {
            continue;
        }
        inserted += file_names.len();
        sqlx::query(
            r#"INSERT INTO map_sync_agent_maps (agent_id, file_name, map_name, size_bytes, modified_at)
               SELECT $1, u.file_name, u.map_name, u.size_bytes, u.modified_at
               FROM UNNEST($2::TEXT[], $3::TEXT[], $4::BIGINT[], $5::TIMESTAMPTZ[])
                    AS u(file_name, map_name, size_bytes, modified_at)"#,
        )
        .bind(agent.id)
        .bind(&file_names)
        .bind(&map_names)
        .bind(&sizes)
        .bind(&modified)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE map_sync_agents SET last_inventory_at = now() WHERE id = $1")
        .bind(agent.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(inserted)
}

pub async fn claim_agent_tasks(
    db: &Database,
    agent: &MapSyncAgent,
    limit: i64,
) -> anyhow::Result<Vec<MapSyncTask>> {
    let limit = limit.clamp(1, 50);
    sqlx::query_as::<_, MapSyncTask>(
        r#"WITH picked AS (
             SELECT id
             FROM map_sync_tasks
             WHERE agent_id = $1
               AND (
                 status = 'pending'
                 OR (status = 'running' AND updated_at < now() - interval '30 minutes')
               )
             ORDER BY created_at ASC
             LIMIT $2
           )
           UPDATE map_sync_tasks t
           SET status = 'running', updated_at = now()
           FROM picked
           WHERE t.id = picked.id
           RETURNING t.id, t.agent_id, NULL::TEXT AS agent_name, NULL::TEXT AS target_type,
                     t.map_name, t.file_name, t.source_url, t.source_size_bytes,
                     t.source_modified_at, t.status, t.reason, t.error, t.created_at, t.updated_at"#,
    )
    .bind(agent.id)
    .bind(limit)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn agent_map_pool(db: &Database, _agent: &MapSyncAgent) -> anyhow::Result<Vec<String>> {
    cached_remote_map_names(db).await
}

pub async fn report_task_result(
    db: &Database,
    agent: &MapSyncAgent,
    task_id: Uuid,
    input: AgentTaskReportInput,
) -> anyhow::Result<()> {
    let status = input.status.trim();
    anyhow::ensure!(matches!(status, "succeeded" | "failed"), "任务状态只能是 succeeded 或 failed");

    let mut tx = db.pool.begin().await?;
    let task: Option<(String, String)> =
        sqlx::query_as("SELECT file_name, map_name FROM map_sync_tasks WHERE id = $1 AND agent_id = $2")
            .bind(task_id)
            .bind(agent.id)
            .fetch_optional(&mut *tx)
            .await?;
    let (file_name, map_name) = task.ok_or_else(|| anyhow::anyhow!("任务不存在"))?;

    sqlx::query(
        r#"UPDATE map_sync_tasks
           SET status = $3, error = $4, updated_at = now()
           WHERE id = $1 AND agent_id = $2"#,
    )
    .bind(task_id)
    .bind(agent.id)
    .bind(status)
    .bind(input.error)
    .execute(&mut *tx)
    .await?;

    if status == "succeeded" {
        sqlx::query(
            r#"INSERT INTO map_sync_agent_maps (agent_id, file_name, map_name, size_bytes, modified_at, reported_at)
               VALUES ($1, $2, $3, $4, $5, now())
               ON CONFLICT (agent_id, file_name) DO UPDATE SET
                 map_name = EXCLUDED.map_name,
                 size_bytes = EXCLUDED.size_bytes,
                 modified_at = EXCLUDED.modified_at,
                 reported_at = now()"#,
        )
        .bind(agent.id)
        .bind(file_name)
        .bind(map_name)
        .bind(input.size_bytes.unwrap_or(0).max(0))
        .bind(input.modified_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn ensure_config_row(db: &Database) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO map_sync_config (id)
           VALUES (true)
           ON CONFLICT (id) DO NOTHING"#,
    )
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn list_agents(db: &Database) -> anyhow::Result<Vec<MapSyncAgent>> {
    sqlx::query_as::<_, MapSyncAgent>(
        r#"SELECT id, name, target_type, token, enabled, last_seen_at, last_inventory_at, created_at
           FROM map_sync_agents
           ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn list_enabled_agents(db: &Database) -> anyhow::Result<Vec<MapSyncAgent>> {
    sqlx::query_as::<_, MapSyncAgent>(
        r#"SELECT id, name, target_type, token, enabled, last_seen_at, last_inventory_at, created_at
           FROM map_sync_agents
           WHERE enabled = true
           ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn list_recent_tasks(db: &Database) -> anyhow::Result<Vec<MapSyncTask>> {
    sqlx::query_as::<_, MapSyncTask>(
        r#"SELECT t.id, t.agent_id, a.name AS agent_name, a.target_type,
                  t.map_name, t.file_name, t.source_url, t.source_size_bytes,
                  t.source_modified_at, t.status, t.reason, t.error, t.created_at, t.updated_at
           FROM map_sync_tasks t
           JOIN map_sync_agents a ON a.id = t.agent_id
           ORDER BY t.created_at DESC
           LIMIT 100"#,
    )
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn stale_reason(
    db: &Database,
    agent_id: Uuid,
    source: &RemoteFile,
) -> anyhow::Result<Option<String>> {
    let local: Option<(i64, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT size_bytes, modified_at FROM map_sync_agent_maps WHERE agent_id = $1 AND file_name = $2")
            .bind(agent_id)
            .bind(&source.file_name)
            .fetch_optional(&db.pool)
            .await?;
    let Some((size, modified_at)) = local else {
        return Ok(Some("目标缺失".to_string()));
    };
    if let Some(source_size) = source.size {
        if size != source_size {
            return Ok(Some("目标文件大小不一致".to_string()));
        }
    }
    if let (Some(source_modified), Some(local_modified)) = (source.last_modified, modified_at) {
        if local_modified < source_modified {
            return Ok(Some("目标文件过旧".to_string()));
        }
    }
    Ok(None)
}

async fn has_active_task(db: &Database, agent_id: Uuid, file_name: &str) -> anyhow::Result<bool> {
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM map_sync_tasks WHERE agent_id = $1 AND file_name = $2 AND status IN ('pending', 'running') LIMIT 1",
    )
    .bind(agent_id)
    .bind(file_name)
    .fetch_optional(&db.pool)
    .await?;
    Ok(exists.is_some())
}

async fn create_task(
    db: &Database,
    agent_id: Uuid,
    map_name: &str,
    source: &RemoteFile,
    reason: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO map_sync_tasks (
             agent_id, map_name, file_name, source_url, source_size_bytes, source_modified_at, reason
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(agent_id)
    .bind(map_name)
    .bind(&source.file_name)
    .bind(&source.url)
    .bind(source.size)
    .bind(source.last_modified)
    .bind(reason)
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn cached_remote_map_names(db: &Database) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT map_name FROM map_sync_remote_maps ORDER BY map_name ASC",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(|row| row.0).collect())
}

async fn persist_remote_map_cache(
    db: &Database,
    desired_map_names: &[String],
    remote_maps: &HashMap<String, RemoteMapSet>,
) -> anyhow::Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM map_sync_remote_maps")
        .execute(&mut *tx)
        .await?;

    for chunk in desired_map_names.chunks(500) {
        let map_names = chunk
            .iter()
            .map(|map_name| map_name.as_str())
            .collect::<Vec<_>>();
        let has_bsp = chunk
            .iter()
            .map(|map_name| remote_maps.get(map_name).and_then(|files| files.raw.as_ref()).is_some())
            .collect::<Vec<_>>();
        let has_bsp_bz2 = chunk
            .iter()
            .map(|map_name| {
                remote_maps
                    .get(map_name)
                    .and_then(|files| files.compressed.as_ref())
                    .is_some()
            })
            .collect::<Vec<_>>();
        sqlx::query(
            r#"INSERT INTO map_sync_remote_maps (map_name, has_bsp, has_bsp_bz2)
               SELECT u.map_name, u.has_bsp, u.has_bsp_bz2
               FROM UNNEST($1::TEXT[], $2::BOOLEAN[], $3::BOOLEAN[])
                    AS u(map_name, has_bsp, has_bsp_bz2)"#,
        )
        .bind(&map_names)
        .bind(&has_bsp)
        .bind(&has_bsp_bz2)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

async fn load_map_pool_names(map_pool_url: &str) -> anyhow::Result<Vec<String>> {
    let response = http_client()
        .get(map_pool_url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "地图池 API 返回状态 {}",
        response.status()
    );
    let rows: Vec<KzMapItem> = response.json().await?;
    let mut names = rows
        .into_iter()
        .filter(|item| item.difficulty.unwrap_or(0) >= 1 && item.difficulty.unwrap_or(0) <= 7)
        .filter_map(|item| normalize_map_name(&item.name))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    anyhow::ensure!(!names.is_empty(), "地图池 API 未返回可用地图");
    Ok(names)
}

async fn persist_check_result(
    db: &Database,
    result: &anyhow::Result<MapSyncCheckResult>,
) -> anyhow::Result<()> {
    match result {
        Ok(summary) => {
            let status = "success";
            let error = if summary.unavailable_files > 0 {
                Some(format!("{} 个目标缺少可用源文件", summary.unavailable_files))
            } else {
                None
            };
            sqlx::query(
                r#"UPDATE map_sync_config
                   SET last_checked_at = now(), last_status = $1, last_error = $2, updated_at = now()
                   WHERE id = true"#,
            )
            .bind(status)
            .bind(error)
            .execute(&db.pool)
            .await?;
        }
        Err(error) => {
            sqlx::query(
                r#"UPDATE map_sync_config
                   SET last_checked_at = now(), last_status = 'failed', last_error = $1, updated_at = now()
                   WHERE id = true"#,
            )
            .bind(error.to_string())
            .execute(&db.pool)
            .await?;
        }
    }
    Ok(())
}

async fn load_remote_maps(source_urls: &[String]) -> anyhow::Result<HashMap<String, RemoteMapSet>> {
    let href_regex = Regex::new(r#"href=["']([^"']+)["']"#)?;
    let mut maps: HashMap<String, RemoteMapSet> = HashMap::new();
    let mut had_success = false;

    for source_url in source_urls {
        let base_url = normalize_source_url(source_url);
        let response = match http_client()
            .get(&base_url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(url = %base_url, %error, "地图源目录请求失败");
                continue;
            }
        };
        if !response.status().is_success() {
            tracing::warn!(url = %base_url, status = %response.status(), "地图源目录返回非成功状态");
            continue;
        }

        let body = response.text().await.unwrap_or_default();
        had_success = true;
        for capture in href_regex.captures_iter(&body) {
            let Some(href) = capture.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let Some(file_name) = safe_map_file_name(href) else {
                continue;
            };
            let Some(map_name) = normalize_map_name(&file_name) else {
                continue;
            };
            let file_url = join_url(&base_url, href)
                .unwrap_or_else(|| format!("{}{}", base_url, file_name));
            let entry = maps.entry(map_name).or_insert(RemoteMapSet {
                raw: None,
                compressed: None,
            });
            if file_name.to_ascii_lowercase().ends_with(".bsp.bz2") {
                if entry.compressed.is_none() {
                    entry.compressed = Some(RemoteFile {
                        file_name,
                        url: file_url,
                        size: None,
                        last_modified: None,
                    });
                }
            } else if entry.raw.is_none() {
                entry.raw = Some(RemoteFile {
                    file_name,
                    url: file_url,
                    size: None,
                    last_modified: None,
                });
            }
        }
    }

    anyhow::ensure!(had_success, "所有地图下载源都无法访问");
    anyhow::ensure!(!maps.is_empty(), "地图下载源中未解析到 .bsp 或 .bsp.bz2 文件");
    Ok(maps)
}

async fn ensure_remote_meta(source: &RemoteFile) -> anyhow::Result<RemoteFile> {
    let response = http_client().head(&source.url).send().await?;
    anyhow::ensure!(
        response.status().is_success(),
        "远程文件返回状态 {}",
        response.status()
    );
    let size = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok());
    let last_modified = response
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
        .map(|value| value.with_timezone(&Utc));

    Ok(RemoteFile {
        file_name: source.file_name.clone(),
        url: source.url.clone(),
        size,
        last_modified,
    })
}

fn select_sources(target_type: &str, remote: &RemoteMapSet) -> Vec<RemoteFile> {
    match target_type {
        "game" => remote.raw.iter().cloned().collect(),
        "download" => {
            let mut sources = Vec::new();
            if let Some(raw) = remote.raw.clone() {
                sources.push(raw);
            }
            if let Some(compressed) = remote.compressed.clone() {
                sources.push(compressed);
            }
            sources
        }
        _ => Vec::new(),
    }
}

fn missing_required_source_count(target_type: &str, remote: &RemoteMapSet) -> usize {
    match target_type {
        "game" => usize::from(remote.raw.is_none()),
        "download" => usize::from(remote.raw.is_none()) + usize::from(remote.compressed.is_none()),
        _ => 1,
    }
}

fn normalize_urls(urls: Vec<String>) -> Vec<String> {
    let mut values: Vec<String> = urls
        .into_iter()
        .map(|value| normalize_source_url(&value))
        .filter(|value| !value.is_empty())
        .collect();
    if values.is_empty() {
        values = DEFAULT_SOURCE_URLS
            .iter()
            .map(|value| value.to_string())
            .collect();
    }
    values.sort();
    values.dedup();
    values
}

fn normalize_source_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

fn safe_map_file_name(value: &str) -> Option<String> {
    let file_name = value
        .split('?')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()?;
    if file_name.contains("..") {
        return None;
    }
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".bsp") || lower.ends_with(".bsp.bz2") {
        Some(file_name.to_string())
    } else {
        None
    }
}

fn normalize_map_name(value: &str) -> Option<String> {
    let mut name = value.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".bsp.bz2") {
        name.truncate(name.len() - ".bsp.bz2".len());
    } else if lower.ends_with(".bsp") {
        name.truncate(name.len() - ".bsp".len());
    }
    let normalized = name.trim().to_string();
    if normalized.is_empty() || normalized.contains('/') || normalized.contains('\\') {
        None
    } else {
        Some(normalized)
    }
}

fn join_url(base_url: &str, href: &str) -> Option<String> {
    reqwest::Url::parse(base_url)
        .ok()
        .and_then(|base| base.join(href).ok())
        .map(|url| url.to_string())
}
