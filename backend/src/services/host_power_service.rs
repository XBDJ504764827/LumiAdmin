//! 宿主机 Agent 电源控制：安装口令、Agent 注册/心跳、电源任务下发/领取/回写。
//!
//! 口令一律只存 SHA-256 哈希；Agent 与 Rust 版 LumiServerAgent 的接口契约对齐：
//! heartbeat `{hostname, instances}`、poll `{max}`、result `{exit_code, output, timed_out}`。

use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) const INSTALL_TOKEN_TTL_SECS: i64 = 15 * 60;
pub(crate) const AGENT_ONLINE_SECS: i64 = 60;
pub(crate) const JOB_OUTPUT_MAX_CHARS: usize = 8000;
const JOB_TAKE_LIMIT_MAX: i64 = 10;

pub(crate) const ACTION_RESTART: &str = "restart";
pub(crate) const ACTION_FORCE_RESTART: &str = "force-restart";
pub(crate) const ACTION_START: &str = "start";
pub(crate) const ACTION_STOP: &str = "stop";

/// 下发给 Agent 执行的动作：强制重启在执行层等同于重启（强制体现在下发策略，不在执行）。
pub(crate) fn exec_action(action: &str) -> &'static str {
    if action == ACTION_FORCE_RESTART {
        ACTION_RESTART
    } else {
        // 调用方已校验 action 合法，这里兜底原样返回
        match action {
            ACTION_START => ACTION_START,
            ACTION_STOP => ACTION_STOP,
            _ => ACTION_RESTART,
        }
    }
}

pub(crate) fn is_valid_action(action: &str) -> bool {
    matches!(
        action,
        ACTION_RESTART | ACTION_FORCE_RESTART | ACTION_START | ACTION_STOP
    )
}

fn generate_token() -> String {
    Uuid::new_v4().simple().to_string()
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: Uuid,
    pub hostname: String,
    pub lgsm_dir: String,
    pub instances: serde_json::Value,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AgentStatus {
    pub id: Uuid,
    pub hostname: String,
    pub lgsm_dir: String,
    pub instances: serde_json::Value,
    pub online: bool,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub server_id: Uuid,
    pub host_agent_id: Option<Uuid>,
    pub action: String,
    pub status: String,
    pub requested_by: Option<Uuid>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub output: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PendingJob {
    pub job_id: Uuid,
    pub server_id: Uuid,
    pub instance: String,
    pub action: String,
}

pub struct InstallTokenInfo {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub struct RegisteredAgent {
    pub agent_id: Uuid,
    pub agent_token: String,
}

// ---------------------------------------------------------------------------
// 管理侧
// ---------------------------------------------------------------------------

pub async fn create_install_token(
    db: &Database,
    created_by: Uuid,
) -> anyhow::Result<InstallTokenInfo> {
    let token = generate_token();
    let expires_at = Utc::now() + chrono::Duration::seconds(INSTALL_TOKEN_TTL_SECS);
    sqlx::query(
        r#"INSERT INTO host_install_tokens (id, token_hash, created_by, expires_at)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(Uuid::new_v4())
    .bind(token_hash(&token))
    .bind(created_by)
    .bind(expires_at)
    .execute(&db.pool)
    .await?;
    Ok(InstallTokenInfo { token, expires_at })
}

pub async fn list_agents(db: &Database) -> anyhow::Result<Vec<AgentStatus>> {
    let rows: Vec<AgentRow> = sqlx::query_as(
        r#"SELECT id, hostname, lgsm_dir, instances, last_seen_at, created_at
           FROM host_agents ORDER BY created_at ASC"#,
    )
    .fetch_all(&db.pool)
    .await?;
    let now = Utc::now();
    Ok(rows
        .into_iter()
        .map(|row| {
            let online = row
                .last_seen_at
                .map(|seen| now.signed_duration_since(seen).num_seconds() <= AGENT_ONLINE_SECS)
                .unwrap_or(false);
            AgentStatus {
                id: row.id,
                hostname: row.hostname,
                lgsm_dir: row.lgsm_dir,
                instances: row.instances,
                online,
                last_seen_at: row.last_seen_at.map(|v| v.to_rfc3339()),
            }
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct ServerBinding {
    host_agent_id: Option<Uuid>,
    lgsm_instance: Option<String>,
}

pub async fn create_power_job(
    db: &Database,
    server_id: Uuid,
    action: &str,
    requested_by: Uuid,
) -> anyhow::Result<JobRow> {
    let action = action.trim().to_lowercase();
    anyhow::ensure!(
        is_valid_action(&action),
        "action 只能为 restart/start/stop/force-restart"
    );

    let binding: Option<ServerBinding> =
        sqlx::query_as(r#"SELECT host_agent_id, lgsm_instance FROM servers WHERE id = $1"#)
            .bind(server_id)
            .fetch_optional(&db.pool)
            .await?;
    let binding = binding.ok_or_else(|| anyhow::anyhow!("服务器不存在"))?;
    let agent_id = binding
        .host_agent_id
        .ok_or_else(|| anyhow::anyhow!("该服务器未绑定 Agent，无法下发电源指令"))?;
    let instance = binding.lgsm_instance.unwrap_or_default();
    anyhow::ensure!(
        !instance.trim().is_empty(),
        "该服务器未填写 LGSM 实例名，无法下发电源指令"
    );

    let active: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM power_jobs
           WHERE server_id = $1 AND status IN ('pending', 'running')"#,
    )
    .bind(server_id)
    .fetch_one(&db.pool)
    .await?;
    anyhow::ensure!(active.0 == 0, "该服务器已有执行中的电源任务，请稍后再试");

    let job: JobRow = sqlx::query_as(
        r#"INSERT INTO power_jobs (id, server_id, host_agent_id, action, requested_by)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, server_id, host_agent_id, action, status, requested_by,
                     exit_code, timed_out, output, created_at, started_at, finished_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(server_id)
    .bind(agent_id)
    .bind(&action)
    .bind(requested_by)
    .fetch_one(&db.pool)
    .await?;
    Ok(job)
}

pub async fn list_server_jobs(
    db: &Database,
    server_id: Uuid,
    limit: i64,
) -> anyhow::Result<Vec<JobRow>> {
    let limit = limit.clamp(1, 50);
    let rows: Vec<JobRow> = sqlx::query_as(
        r#"SELECT id, server_id, host_agent_id, action, status, requested_by,
                  exit_code, timed_out, output, created_at, started_at, finished_at
           FROM power_jobs WHERE server_id = $1 ORDER BY created_at DESC LIMIT $2"#,
    )
    .bind(server_id)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Agent 侧
// ---------------------------------------------------------------------------

/// 校验安装口令有效（存在、未使用、未过期），只读不消费；
/// 供文件下载接口在安装阶段放行，消费只发生在注册时。
pub async fn check_install_token_valid(db: &Database, token: &str) -> anyhow::Result<()> {
    let token = token.trim();
    anyhow::ensure!(!token.is_empty(), "安装口令不能为空");
    let row: Option<(DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT expires_at, used_at FROM host_install_tokens WHERE token_hash = $1"#,
    )
    .bind(token_hash(token))
    .fetch_optional(&db.pool)
    .await?;
    let (expires_at, used_at) = row.ok_or_else(|| anyhow::anyhow!("安装口令无效"))?;
    anyhow::ensure!(used_at.is_none(), "安装口令已使用");
    anyhow::ensure!(expires_at > Utc::now(), "安装口令已过期");
    Ok(())
}

pub async fn register_agent(
    db: &Database,
    install_token: &str,
    hostname: &str,
    lgsm_dir: &str,
    instances: &[String],
) -> anyhow::Result<RegisteredAgent> {
    let token = install_token.trim();
    anyhow::ensure!(!token.is_empty(), "install_token 不能为空");
    let hash = token_hash(token);

    let mut tx = db.pool.begin().await?;
    let row: Option<(Uuid, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT id, expires_at, used_at FROM host_install_tokens WHERE token_hash = $1"#,
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    let (token_id, expires_at, used_at) = row.ok_or_else(|| anyhow::anyhow!("安装口令无效"))?;
    anyhow::ensure!(used_at.is_none(), "安装口令已使用");
    anyhow::ensure!(expires_at > Utc::now(), "安装口令已过期");

    let agent_id = Uuid::new_v4();
    let agent_token = generate_token();
    let instances_json =
        serde_json::to_value(instances).unwrap_or(serde_json::Value::Array(vec![]));
    sqlx::query(
        r#"INSERT INTO host_agents (id, hostname, lgsm_dir, instances, token_hash, last_seen_at)
           VALUES ($1, $2, $3, $4, $5, now())"#,
    )
    .bind(agent_id)
    .bind(hostname.trim())
    .bind(lgsm_dir.trim())
    .bind(&instances_json)
    .bind(token_hash(&agent_token))
    .execute(&mut *tx)
    .await?;
    sqlx::query(r#"UPDATE host_install_tokens SET used_at = now() WHERE id = $1"#)
        .bind(token_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(RegisteredAgent {
        agent_id,
        agent_token,
    })
}

pub async fn authenticate_agent(db: &Database, token: &str) -> anyhow::Result<AgentRow> {
    let token = token.trim();
    anyhow::ensure!(!token.is_empty(), "Agent 口令不能为空");
    // Agent 数量极少（每宿主机一个），一次查出后在内存中常量时间比对；
    // token 明文永不入库，哈希等长，比对不提前返回。
    #[derive(sqlx::FromRow)]
    struct AgentAuthRow {
        id: Uuid,
        hostname: String,
        lgsm_dir: String,
        instances: serde_json::Value,
        last_seen_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        token_hash: String,
    }
    let rows: Vec<AgentAuthRow> = sqlx::query_as(
        r#"SELECT id, hostname, lgsm_dir, instances, last_seen_at, created_at, token_hash
           FROM host_agents"#,
    )
    .fetch_all(&db.pool)
    .await?;
    let challenge = token_hash(token);
    let mut matched: Option<AgentRow> = None;
    for row in rows {
        if constant_time_eq(&row.token_hash, &challenge) {
            matched = Some(AgentRow {
                id: row.id,
                hostname: row.hostname,
                lgsm_dir: row.lgsm_dir,
                instances: row.instances,
                last_seen_at: row.last_seen_at,
                created_at: row.created_at,
            });
        }
    }
    matched.ok_or_else(|| anyhow::anyhow!("Agent 口令无效"))
}

pub async fn heartbeat(
    db: &Database,
    agent_id: Uuid,
    hostname: &str,
    instances: &[String],
) -> anyhow::Result<()> {
    let instances_json =
        serde_json::to_value(instances).unwrap_or(serde_json::Value::Array(vec![]));
    sqlx::query(
        r#"UPDATE host_agents SET hostname = $2, instances = $3, last_seen_at = now() WHERE id = $1"#,
    )
    .bind(agent_id)
    .bind(hostname.trim())
    .bind(&instances_json)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn poll_jobs(db: &Database, agent_id: Uuid, max: i64) -> anyhow::Result<Vec<PendingJob>> {
    let max = max.clamp(1, JOB_TAKE_LIMIT_MAX);
    let mut tx = db.pool.begin().await?;
    let rows: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        r#"SELECT j.id, j.server_id, s.lgsm_instance, j.action
           FROM power_jobs j
           JOIN servers s ON s.id = j.server_id
           WHERE j.status = 'pending' AND s.host_agent_id = $1
           ORDER BY j.created_at ASC LIMIT $2"#,
    )
    .bind(agent_id)
    .bind(max)
    .fetch_all(&mut *tx)
    .await?;
    let mut jobs = Vec::with_capacity(rows.len());
    for (job_id, server_id, instance, action) in rows {
        sqlx::query(
            r#"UPDATE power_jobs SET status = 'running', started_at = now()
               WHERE id = $1 AND status = 'pending'"#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        jobs.push(PendingJob {
            job_id,
            server_id,
            instance,
            action: exec_action(&action).to_string(),
        });
    }
    tx.commit().await?;
    Ok(jobs)
}

pub async fn report_result(
    db: &Database,
    agent_id: Uuid,
    job_id: Uuid,
    exit_code: i32,
    output: Option<String>,
    timed_out: bool,
) -> anyhow::Result<JobRow> {
    let output = output.map(|text| {
        let clipped: String = text.chars().take(JOB_OUTPUT_MAX_CHARS).collect();
        clipped
    });
    let status = if timed_out {
        "timeout"
    } else if exit_code == 0 {
        "success"
    } else {
        "failed"
    };
    let job: JobRow = sqlx::query_as(
        r#"UPDATE power_jobs
           SET status = $3, exit_code = $4, timed_out = $5, output = $6, finished_at = now()
           WHERE id = $1 AND host_agent_id = $2 AND status IN ('pending', 'running')
           RETURNING id, server_id, host_agent_id, action, status, requested_by,
                     exit_code, timed_out, output, created_at, started_at, finished_at"#,
    )
    .bind(job_id)
    .bind(agent_id)
    .bind(status)
    .bind(exit_code)
    .bind(timed_out)
    .bind(output)
    .fetch_one(&db.pool)
    .await
    .map_err(|_| anyhow::anyhow!("任务不存在或不属于该 Agent"))?;
    Ok(job)
}

#[derive(Debug, Serialize)]
pub struct PowerOverview {
    pub hosts: i64,
    pub instances: i64,
    pub online: i64,
    pub pending_jobs: i64,
}

pub async fn overview(db: &Database) -> anyhow::Result<PowerOverview> {
    let hosts: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM host_agents"#)
        .fetch_one(&db.pool)
        .await?;
    let online: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM host_agents
           WHERE last_seen_at IS NOT NULL AND last_seen_at > now() - INTERVAL '60 seconds'"#,
    )
    .fetch_one(&db.pool)
    .await?;
    let instances: (Option<i64>,) =
        sqlx::query_as(r#"SELECT SUM(jsonb_array_length(instances)) FROM host_agents"#)
            .fetch_one(&db.pool)
            .await?;
    let pending_jobs: (i64,) =
        sqlx::query_as(r#"SELECT COUNT(*) FROM power_jobs WHERE status IN ('pending', 'running')"#)
            .fetch_one(&db.pool)
            .await?;
    Ok(PowerOverview {
        hosts: hosts.0,
        instances: instances.0.unwrap_or(0),
        online: online.0,
        pending_jobs: pending_jobs.0,
    })
}
