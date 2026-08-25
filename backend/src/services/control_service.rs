use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const INSTALL_TOKEN_TTL_SECS: i64 = 15 * 60;
const AGENT_OFFLINE_SECS: i64 = 60;

#[derive(Debug, Serialize)]
pub struct InstallTokenInfo {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub install_command: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DiscoveryRow {
    pub id: Uuid,
    pub community_id: Uuid,
    pub agent_id: Uuid,
    pub instance_name: String,
    pub instance_path: String,
    pub ip: String,
    pub port: Option<i32>,
    pub server_name: Option<String>,
    pub rcon_password: Option<String>,
    pub status: String,
    pub server_id: Option<Uuid>,
    pub raw: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: Uuid,
    pub community_id: Uuid,
    pub token: String,
    pub hostname: String,
    pub ip: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AgentStatus {
    pub id: Uuid,
    pub community_id: Uuid,
    pub hostname: String,
    pub ip: String,
    pub last_seen_at: Option<String>,
    pub online: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub server_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub community_id: Uuid,
    pub action: String,
    pub status: String,
    pub output: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterAgentInput {
    pub install_token: String,
    pub hostname: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DiscoverCandidate {
    pub instance_name: String,
    pub instance_path: Option<String>,
    pub ip: String,
    pub port: Option<i32>,
    pub server_name: Option<String>,
    pub rcon_password: Option<String>,
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct DiscoverInput {
    pub token: String,
    pub candidates: Vec<DiscoverCandidate>,
}

#[derive(Debug, Deserialize)]
pub struct PollInput {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ResultInput {
    pub token: String,
    pub job_id: Uuid,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PowerInput {
    pub action: String,
}

fn generate_token() -> String {
    Uuid::new_v4().simple().to_string()
}

fn make_install_command(base_url: &str, token: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!(
        "curl -fsSL {}/api/control/install.sh | bash -s -- --url {} --token {}",
        base, base, token
    )
}

pub async fn create_install_token(
    db: &Database,
    community_id: Uuid,
    created_by: Uuid,
    base_url: &str,
) -> anyhow::Result<InstallTokenInfo> {
    let community_exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM communities WHERE id = $1")
            .bind(community_id)
            .fetch_optional(&db.pool)
            .await?;
    anyhow::ensure!(community_exists.is_some(), "社区不存在");

    let token = generate_token();
    let id = Uuid::new_v4();
    let expires_at = Utc::now() + chrono::Duration::seconds(INSTALL_TOKEN_TTL_SECS);
    sqlx::query(
        r#"INSERT INTO control_install_tokens (id, community_id, token, created_by, expires_at)
           VALUES ($1,$2,$3,$4,$5)"#,
    )
    .bind(id)
    .bind(community_id)
    .bind(&token)
    .bind(created_by)
    .bind(expires_at)
    .execute(&db.pool)
    .await?;
    let install_command = make_install_command(base_url, &token);
    Ok(InstallTokenInfo {
        token,
        expires_at,
        install_command,
    })
}

pub async fn register_agent(db: &Database, input: RegisterAgentInput) -> anyhow::Result<AgentRow> {
    let token = input.install_token.trim().to_string();
    anyhow::ensure!(!token.is_empty(), "install_token 不能为空");

    let row: Option<(Uuid, Uuid, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT id, community_id, expires_at, used_at FROM control_install_tokens WHERE token = $1"#,
    )
    .bind(&token)
    .fetch_optional(&db.pool)
    .await?;
    let (install_id, community_id, expires_at, used_at) =
        row.ok_or_else(|| anyhow::anyhow!("安装口令无效"))?;
    anyhow::ensure!(used_at.is_none(), "安装口令已使用");
    anyhow::ensure!(expires_at > Utc::now(), "安装口令已过期");

    let agent_id = Uuid::new_v4();
    let agent_token = generate_token();
    let hostname = input.hostname.unwrap_or_default();
    let ip = input.ip.unwrap_or_default();
    sqlx::query(
        r#"INSERT INTO control_agents (id, community_id, token, hostname, ip, install_token_id)
           VALUES ($1,$2,$3,$4,$5,$6)"#,
    )
    .bind(agent_id)
    .bind(community_id)
    .bind(&agent_token)
    .bind(hostname.trim())
    .bind(ip.trim())
    .bind(install_id)
    .execute(&db.pool)
    .await?;

    sqlx::query(r#"UPDATE control_install_tokens SET used_at = now() WHERE id = $1"#)
        .bind(install_id)
        .execute(&db.pool)
        .await?;

    let agent: AgentRow = sqlx::query_as(
        r#"SELECT id, community_id, token, hostname, ip, last_seen_at, created_at FROM control_agents WHERE id = $1"#,
    )
    .bind(agent_id)
    .fetch_one(&db.pool)
    .await?;
    Ok(agent)
}

pub async fn report_discoveries(
    db: &Database,
    input: DiscoverInput,
) -> anyhow::Result<Vec<DiscoveryRow>> {
    let token = input.token.trim();
    anyhow::ensure!(!token.is_empty(), "token 不能为空");
    let agent: Option<AgentRow> = sqlx::query_as(
        r#"SELECT id, community_id, token, hostname, ip, last_seen_at, created_at FROM control_agents WHERE token = $1"#,
    )
    .bind(token)
    .fetch_optional(&db.pool)
    .await?;
    let agent = agent.ok_or_else(|| anyhow::anyhow!("Agent口令无效"))?;
    sqlx::query(r#"UPDATE control_agents SET last_seen_at = now() WHERE id = $1"#)
        .bind(agent.id)
        .execute(&db.pool)
        .await?;

    // Also sync servers.control_last_seen_at
    sqlx::query(r#"UPDATE servers SET control_last_seen_at = now() WHERE control_agent_id = $1"#)
        .bind(agent.id)
        .execute(&db.pool)
        .await?;

    anyhow::ensure!(!input.candidates.is_empty(), "候选列表不能为空");
    anyhow::ensure!(input.candidates.len() <= 64, "候选数量过多");

    let mut results = Vec::new();
    for cand in input.candidates {
        let instance_name = cand.instance_name.trim().to_string();
        anyhow::ensure!(!instance_name.is_empty(), "instance_name 不能为空");
        anyhow::ensure!(instance_name.len() <= 64, "instance_name 过长");
        let ip = cand.ip.trim().to_string();
        let port = cand.port;
        if let Some(p) = port {
            anyhow::ensure!(p > 0 && p < 65535, "端口无效");
        }
        let server_name = cand
            .server_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let rcon = cand
            .rcon_password
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let instance_path = cand.instance_path.unwrap_or_default();
        let raw = cand
            .raw
            .unwrap_or(serde_json::Value::Object(Default::default()));

        // Check if already exists as pending with same tuple, update
        let existing: Option<(Uuid,)> = sqlx::query_as(
            r#"SELECT id FROM control_discoveries
               WHERE community_id = $1 AND ip = $2 AND COALESCE(port,0) = COALESCE($3,0) AND instance_name = $4 AND status = 'pending'"#,
        )
        .bind(agent.community_id)
        .bind(&ip)
        .bind(port)
        .bind(&instance_name)
        .fetch_optional(&db.pool)
        .await?;
        if let Some((did,)) = existing {
            sqlx::query(
                r#"UPDATE control_discoveries SET agent_id = $2, instance_path=$3, server_name=$4, rcon_password=$5, raw=$6, updated_at=now() WHERE id=$1"#,
            )
            .bind(did)
            .bind(agent.id)
            .bind(&instance_path)
            .bind(&server_name)
            .bind(&rcon)
            .bind(&raw)
            .execute(&db.pool)
            .await?;
            let row: DiscoveryRow = sqlx::query_as(
                r#"SELECT id, community_id, agent_id, instance_name, instance_path, ip, port, server_name, rcon_password, status, server_id, raw, created_at, updated_at FROM control_discoveries WHERE id=$1"#)
                .bind(did).fetch_one(&db.pool).await?;
            results.push(row);
            continue;
        }

        // If server already exists with same ip/port, skip creating discovery? but still report to show confirmed state
        let server_exists: Option<(Uuid,)> = if let Some(p) = port {
            sqlx::query_as(r#"SELECT id FROM servers WHERE ip = $1 AND port = $2 LIMIT 1"#)
                .bind(&ip)
                .bind(p)
                .fetch_optional(&db.pool)
                .await?
        } else {
            None
        };
        if server_exists.is_some() {
            // create a discovery marked as confirmed to indicate already exists? Instead skip.
            continue;
        }

        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO control_discoveries (id, community_id, agent_id, instance_name, instance_path, ip, port, server_name, rcon_password, status, raw)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'pending',$10)"#,
        )
        .bind(id)
        .bind(agent.community_id)
        .bind(agent.id)
        .bind(&instance_name)
        .bind(&instance_path)
        .bind(&ip)
        .bind(port)
        .bind(&server_name)
        .bind(&rcon)
        .bind(&raw)
        .execute(&db.pool)
        .await?;
        let row: DiscoveryRow = sqlx::query_as(
            r#"SELECT id, community_id, agent_id, instance_name, instance_path, ip, port, server_name, rcon_password, status, server_id, raw, created_at, updated_at FROM control_discoveries WHERE id=$1"#,
        )
        .bind(id)
        .fetch_one(&db.pool)
        .await?;
        results.push(row);
    }
    Ok(results)
}

pub async fn list_discoveries(
    db: &Database,
    community_id: Uuid,
) -> anyhow::Result<Vec<DiscoveryRow>> {
    let rows: Vec<DiscoveryRow> = sqlx::query_as(
        r#"SELECT id, community_id, agent_id, instance_name, instance_path, ip, port, server_name, rcon_password, status, server_id, raw, created_at, updated_at
           FROM control_discoveries WHERE community_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(community_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

pub async fn list_agents(db: &Database, community_id: Uuid) -> anyhow::Result<Vec<AgentStatus>> {
    let rows: Vec<AgentRow> = sqlx::query_as(
        r#"SELECT id, community_id, token, hostname, ip, last_seen_at, created_at FROM control_agents WHERE community_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(community_id)
    .fetch_all(&db.pool)
    .await?;
    let now = Utc::now();
    Ok(rows
        .into_iter()
        .map(|r| {
            let online = r
                .last_seen_at
                .map(|t| (now - t).num_seconds() < AGENT_OFFLINE_SECS)
                .unwrap_or(false);
            AgentStatus {
                id: r.id,
                community_id: r.community_id,
                hostname: r.hostname,
                ip: r.ip,
                last_seen_at: r.last_seen_at.map(|t| t.to_rfc3339()),
                online,
            }
        })
        .collect())
}

pub async fn confirm_discoveries(
    db: &Database,
    community_id: Uuid,
    ids: Vec<Uuid>,
) -> anyhow::Result<Vec<Uuid>> {
    anyhow::ensure!(!ids.is_empty(), "请选择要确认的服务器");
    anyhow::ensure!(ids.len() <= 64, "一次最多确认64个");

    let mut created_servers = Vec::new();
    for did in ids {
        let disc: Option<DiscoveryRow> = sqlx::query_as(
            r#"SELECT id, community_id, agent_id, instance_name, instance_path, ip, port, server_name, rcon_password, status, server_id, raw, created_at, updated_at
               FROM control_discoveries WHERE id = $1 AND community_id = $2"#,
        )
        .bind(did)
        .bind(community_id)
        .fetch_optional(&db.pool)
        .await?;
        let disc = disc.ok_or_else(|| anyhow::anyhow!("发现记录不存在"))?;
        anyhow::ensure!(disc.status == "pending", "该记录已处理");
        let port = disc.port.ok_or_else(|| {
            anyhow::anyhow!(format!(
                "实例 {} 缺少端口，请先在游戏服配置中补全port",
                disc.instance_name
            ))
        })?;
        let ip = disc.ip.clone();
        // dedup servers
        let exists: Option<(Uuid,)> =
            sqlx::query_as(r#"SELECT id FROM servers WHERE ip = $1 AND port = $2 LIMIT 1"#)
                .bind(&ip)
                .bind(port)
                .fetch_optional(&db.pool)
                .await?;
        if exists.is_some() {
            sqlx::query(
                r#"UPDATE control_discoveries SET status='skipped', updated_at=now() WHERE id=$1"#,
            )
            .bind(did)
            .execute(&db.pool)
            .await?;
            continue;
        }
        let name = disc
            .server_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| disc.instance_name.clone());
        let rcon = disc
            .rcon_password
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "CHANGE_ME".to_string());
        let report_token = Uuid::new_v4().simple().to_string();
        let server_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players, lgsm_instance, control_agent_id, note)
               VALUES ($1,$2,$3,$4,$5,$6,$7,'untested',ARRAY[]::TEXT[],$8,$9,$10)"#,
        )
        .bind(server_id)
        .bind(community_id)
        .bind(name.trim())
        .bind(&ip)
        .bind(port)
        .bind(rcon.trim())
        .bind(&report_token)
        .bind(&disc.instance_name)
        .bind(disc.agent_id)
        .bind(format!("auto discovered {}", disc.instance_name))
        .execute(&db.pool)
        .await?;
        sqlx::query(r#"UPDATE control_discoveries SET status='confirmed', server_id=$2, updated_at=now() WHERE id=$1"#)
            .bind(did)
            .bind(server_id)
            .execute(&db.pool)
            .await?;
        created_servers.push(server_id);
    }
    Ok(created_servers)
}

pub async fn poll_job(db: &Database, token: &str) -> anyhow::Result<Option<JobRow>> {
    let t = token.trim();
    anyhow::ensure!(!t.is_empty(), "token 不能为空");
    let agent: Option<AgentRow> = sqlx::query_as(
        r#"SELECT id, community_id, token, hostname, ip, last_seen_at, created_at FROM control_agents WHERE token = $1"#,
    )
    .bind(t)
    .fetch_optional(&db.pool)
    .await?;
    let agent = agent.ok_or_else(|| anyhow::anyhow!("Agent口令无效"))?;
    sqlx::query(r#"UPDATE control_agents SET last_seen_at = now() WHERE id = $1"#)
        .bind(agent.id)
        .execute(&db.pool)
        .await?;
    sqlx::query(r#"UPDATE servers SET control_last_seen_at = now() WHERE control_agent_id = $1"#)
        .bind(agent.id)
        .execute(&db.pool)
        .await?;

    let job: Option<JobRow> = sqlx::query_as(
        r#"SELECT id, server_id, agent_id, community_id, action, status, output, created_at, started_at, finished_at
           FROM control_jobs WHERE agent_id = $1 AND status = 'pending' ORDER BY created_at ASC LIMIT 1"#,
    )
    .bind(agent.id)
    .fetch_optional(&db.pool)
    .await?;
    if let Some(j) = job {
        sqlx::query(r#"UPDATE control_jobs SET status='running', started_at=now() WHERE id=$1"#)
            .bind(j.id)
            .execute(&db.pool)
            .await?;
        let updated: JobRow = sqlx::query_as(
            r#"SELECT id, server_id, agent_id, community_id, action, status, output, created_at, started_at, finished_at FROM control_jobs WHERE id=$1"#,
        )
        .bind(j.id)
        .fetch_one(&db.pool)
        .await?;
        // Need to map to agent instance name: fetch server lgsm_instance
        Ok(Some(updated))
    } else {
        Ok(None)
    }
}

pub async fn report_job_result(db: &Database, input: ResultInput) -> anyhow::Result<JobRow> {
    let token = input.token.trim();
    anyhow::ensure!(!token.is_empty(), "token 不能为空");
    let agent: Option<AgentRow> = sqlx::query_as(
        r#"SELECT id, community_id, token, hostname, ip, last_seen_at, created_at FROM control_agents WHERE token = $1"#,
    )
    .bind(token)
    .fetch_optional(&db.pool)
    .await?;
    let agent = agent.ok_or_else(|| anyhow::anyhow!("Agent口令无效"))?;
    let job: Option<JobRow> = sqlx::query_as(
        r#"SELECT id, server_id, agent_id, community_id, action, status, output, created_at, started_at, finished_at FROM control_jobs WHERE id=$1 AND agent_id=$2"#,
    )
    .bind(input.job_id)
    .bind(agent.id)
    .fetch_optional(&db.pool)
    .await?;
    let job = job.ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
    anyhow::ensure!(
        job.status == "running" || job.status == "pending",
        "任务已结束"
    );
    let status = if input.exit_code.unwrap_or(1) == 0 {
        "success"
    } else {
        "failed"
    };
    let output = input
        .output
        .unwrap_or_default()
        .chars()
        .take(8000)
        .collect::<String>();
    sqlx::query(r#"UPDATE control_jobs SET status=$2, output=$3, finished_at=now() WHERE id=$1"#)
        .bind(job.id)
        .bind(status)
        .bind(&output)
        .execute(&db.pool)
        .await?;
    sqlx::query(r#"UPDATE control_agents SET last_seen_at = now() WHERE id=$1"#)
        .bind(agent.id)
        .execute(&db.pool)
        .await?;
    let updated: JobRow = sqlx::query_as(
        r#"SELECT id, server_id, agent_id, community_id, action, status, output, created_at, started_at, finished_at FROM control_jobs WHERE id=$1"#,
    )
    .bind(job.id)
    .fetch_one(&db.pool)
    .await?;
    Ok(updated)
}

pub async fn create_power_job(
    db: &Database,
    server_id: Uuid,
    action: &str,
    requested_by: Uuid,
) -> anyhow::Result<JobRow> {
    let act = action.trim().to_lowercase();
    anyhow::ensure!(
        ["restart", "start", "stop"].contains(&act.as_str()),
        "action 只能为 restart/start/stop"
    );
    let server: Option<(Uuid, Option<Uuid>, Uuid, Option<String>)> = sqlx::query_as(
        r#"SELECT id, control_agent_id, community_id, lgsm_instance FROM servers WHERE id = $1"#,
    )
    .bind(server_id)
    .fetch_optional(&db.pool)
    .await?;
    let (sid, agent_id, community_id, lgsm_instance) =
        server.ok_or_else(|| anyhow::anyhow!("服务器不存在"))?;
    let agent_id = agent_id
        .ok_or_else(|| anyhow::anyhow!("该服务器未绑定控制 Agent，请先安装并完成发现确认"))?;
    anyhow::ensure!(
        lgsm_instance.is_some() || true,
        "该服务器缺少 lgsm 实例信息"
    );

    // rate limit 5 minutes per server per action
    let recent: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT id FROM control_jobs WHERE server_id=$1 AND action=$2 AND created_at > now() - interval '5 minutes' LIMIT 1"#,
    )
    .bind(sid)
    .bind(&act)
    .fetch_optional(&db.pool)
    .await?;
    if recent.is_some() {
        anyhow::bail!("操作过于频繁，请5分钟后再试");
    }

    // also ensure no pending/running job for same server
    let pending: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT id FROM control_jobs WHERE server_id=$1 AND status IN ('pending','running') LIMIT 1"#,
    )
    .bind(sid)
    .fetch_optional(&db.pool)
    .await?;
    if pending.is_some() {
        anyhow::bail!("该服务器已有待执行任务，请稍后再试");
    }

    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO control_jobs (id, server_id, agent_id, community_id, action, status, requested_by)
           VALUES ($1,$2,$3,$4,$5,'pending',$6)"#,
    )
    .bind(id)
    .bind(sid)
    .bind(agent_id)
    .bind(community_id)
    .bind(&act)
    .bind(requested_by)
    .execute(&db.pool)
    .await?;
    let job: JobRow = sqlx::query_as(
        r#"SELECT id, server_id, agent_id, community_id, action, status, output, created_at, started_at, finished_at FROM control_jobs WHERE id=$1"#,
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await?;
    Ok(job)
}

pub async fn list_jobs_for_server(db: &Database, server_id: Uuid) -> anyhow::Result<Vec<JobRow>> {
    let rows: Vec<JobRow> = sqlx::query_as(
        r#"SELECT id, server_id, agent_id, community_id, action, status, output, created_at, started_at, finished_at
           FROM control_jobs WHERE server_id=$1 ORDER BY created_at DESC LIMIT 20"#,
    )
    .bind(server_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

pub fn install_script_content() -> String {
    // Bash install script running as csgoserver user, no sudo.
    r#"#!/usr/bin/env bash
set -euo pipefail
# LumiAdmin LGSM Control Agent Installer (user-mode, no root)
# Usage: curl -fsSL https://DOMAIN/api/control/install.sh | bash -s -- --url https://DOMAIN --token INSTALL_TOKEN

URL=""
TOKEN=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --url) URL="$2"; shift 2;;
    --token) TOKEN="$2"; shift 2;;
    *) echo "unknown arg $1"; exit 1;;
  esac
done
if [[ -z "$URL" || -z "$TOKEN" ]]; then
  echo "usage: $0 --url https://DOMAIN --token INSTALL_TOKEN"
  exit 1
fi
URL="${URL%/}"
echo "[lumi-agent] registering with $URL ..."
HOSTNAME="$(hostname -f 2>/dev/null || hostname)"
IP="$(hostname -I 2>/dev/null | awk '{print $1}')"
if [[ -z "$IP" ]]; then IP="127.0.0.1"; fi

REG_JSON=$(printf '{"install_token":"%s","hostname":"%s","ip":"%s"}' "$TOKEN" "$HOSTNAME" "$IP")
RESP=$(curl -fsSL -X POST "$URL/api/plugin/control/register" -H "Content-Type: application/json" -d "$REG_JSON" || true)
AGENT_TOKEN=$(echo "$RESP" | sed -n 's/.*"token"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
COMMUNITY=$(echo "$RESP" | sed -n 's/.*"community_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
if [[ -z "$AGENT_TOKEN" ]]; then
  echo "[lumi-agent] register failed: $RESP"
  exit 1
fi
echo "[lumi-agent] registered agent $AGENT_TOKEN community $COMMUNITY"

AGENT_DIR="$HOME/lumi-agent"
mkdir -p "$AGENT_DIR"
printf '%s' "$AGENT_TOKEN" > "$AGENT_DIR/agent.token"
printf '%s' "$URL" > "$AGENT_DIR/agent.url"
printf '%s' "$COMMUNITY" > "$AGENT_DIR/community.id"
chmod 600 "$AGENT_DIR/agent.token"

# --- discovery ---
echo "[lumi-agent] discovering LGSM instances ..."
CANDIDATES="["; FIRST=1
# find csgoserver executables in $HOME (maxdepth 1)
for f in "$HOME"/csgoserver "$HOME"/csgoserver-*; do
  [[ -e "$f" ]] || continue
  [[ -f "$f" ]] || continue
  [[ -x "$f" ]] || continue
  bn=$(basename "$f")
  # skip non-lgsm files
  if [[ ! "$bn" =~ ^csgoserver ]]; then continue; fi
  # gather port: search lgsm config
  PORT=""
  for cfg in "$HOME/lgsm/config-lgsm/$bn" "$HOME/lgsm/config-lgsm/$bn.cfg" "$HOME/lgsm/config-lgsm/$bn/common.cfg" "$HOME/lgsm/config-lgsm/$bn/csgoserver.cfg"; do
    [[ -f "$cfg" ]] || continue
    v=$(grep -hE 'port[[:space:]]*=[[:space:]]*"?[0-9]+"?' "$cfg" 2>/dev/null | head -1 | grep -oE '[0-9]{2,5}' | head -1 || true)
    [[ -n "$v" ]] && PORT="$v" && break
  done
  if [[ -z "$PORT" ]]; then
    # broad search under lgsm/config-lgsm/$bn
    if [[ -d "$HOME/lgsm/config-lgsm/$bn" ]]; then
      v=$(grep -RhE 'port[[:space:]]*=[[:space:]]*"?[0-9]+"?' "$HOME/lgsm/config-lgsm/$bn" 2>/dev/null | head -1 | grep -oE '[0-9]{2,5}' | head -1 || true)
      [[ -n "$v" ]] && PORT="$v"
    fi
  fi
  # rcon & hostname: search under serverfiles
  RCON=""; SNAME=""
  for base in "$HOME/serverfiles/csgo/cfg" "$HOME/serverfiles/csgoserver/csgo/cfg" "$HOME/serverfiles/csgo" "$HOME/serverfiles"; do
    [[ -d "$base" ]] || continue
    # exact instance cfg priority: $bn.cfg
    for cand in "$base/$bn.cfg" "$base/csgoserver.cfg"; do
      [[ -f "$cand" ]] || continue
      r=$(grep -hE 'rcon_password[[:space:]]+"?[^"]+"?' "$cand" 2>/dev/null | head -1 | sed -E 's/.*rcon_password[[:space:]]*"?"([^"]+)"?.*/\1/' || true)
      [[ -n "$r" && -z "$RCON" ]] && RCON="$r"
      s=$(grep -hE 'hostname[[:space:]]+"[^"]+"' "$cand" 2>/dev/null | head -1 | sed -E 's/.*hostname[[:space:]]*"([^"]+)".*/\1/' || true)
      [[ -n "$s" && -z "$SNAME" ]] && SNAME="$s"
    done
    # fallback broad grep limited
    if [[ -z "$RCON" ]]; then
      r=$(grep -RhE 'rcon_password' "$base" 2>/dev/null | head -1 | sed -E 's/.*rcon_password[[:space:]]*"?"([^"]+)"?.*/\1/' || true)
      [[ -n "$r" ]] && RCON="$r"
    fi
  done
  # json escape
  esc() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'; }
  bn_e=$(esc "$bn"); ip_e=$(esc "$IP"); path_e=$(esc "$f")
  rcon_e=$(esc "$RCON"); sname_e=$(esc "$SNAME")
  PORT_JSON="null"; [[ -n "$PORT" ]] && PORT_JSON="$PORT"
  RCON_JSON="null"; [[ -n "$RCON" ]] && RCON_JSON="\"$rcon_e\""
  SNAME_JSON="null"; [[ -n "$SNAME" ]] && SNAME_JSON="\"$sname_e\""
  if [[ $FIRST -eq 0 ]]; then CANDIDATES+=", "; fi
  CANDIDATES+="{\"instance_name\":\"$bn_e\",\"instance_path\":\"$path_e\",\"ip\":\"$ip_e\",\"port\":$PORT_JSON,\"server_name\":$SNAME_JSON,\"rcon_password\":$RCON_JSON}"
  FIRST=0
done
CANDIDATES+="]"
if [[ "$CANDIDATES" == "[]" ]]; then
  echo "[lumi-agent] no LGSM instances found in $HOME"
else
  echo "[lumi-agent] candidates: $CANDIDATES"
  DIS_JSON=$(printf '{"token":"%s","candidates":%s}' "$AGENT_TOKEN" "$CANDIDATES")
  curl -fsSL -X POST "$URL/api/plugin/control/discover" -H "Content-Type: application/json" -d "$DIS_JSON" || echo "[lumi-agent] discover post failed"
fi

# --- install poll agent (lumi-agent.sh) ---
cat > "$AGENT_DIR/lumi-agent.sh" <<'EOSH'
#!/usr/bin/env bash
set -uo pipefail
AGENT_DIR="$HOME/lumi-agent"
TOKEN=$(cat "$AGENT_DIR/agent.token" 2>/dev/null || echo "")
URL=$(cat "$AGENT_DIR/agent.url" 2>/dev/null || echo "")
if [[ -z "$TOKEN" || -z "$URL" ]]; then echo "[lumi-agent] missing token/url"; exit 1; fi
while true; do
  RESP=$(curl -fsSL -m 10 -X POST "$URL/api/plugin/control/poll" -H "Content-Type: application/json" -d "{\"token\":\"$TOKEN\"}" 2>/dev/null || true)
  JOB_ID=$(echo "$RESP" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
  ACTION=$(echo "$RESP" | sed -n 's/.*"action"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
  SERVER_ID=$(echo "$RESP" | sed -n 's/.*"server_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
  LGSM=$(echo "$RESP" | sed -n 's/.*"lgsm_instance"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
  if [[ -n "$JOB_ID" && -n "$ACTION" ]]; then
    echo "[lumi-agent] job $JOB_ID action $ACTION lgsm $LGSM"
    OUT=""; CODE=0
    if [[ -z "$LGSM" ]]; then LGSM="csgoserver"; fi
    # execute from HOME
    set +e
    OUT=$(bash -c "cd \"$HOME\" && ./\"$LGSM\" $ACTION" 2>&1)
    CODE=$?
    set -e
    # truncate
    OUT_TRIM=$(printf '%s' "$OUT" | head -c 7000 | sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/g' | tr -d '\r' | paste -sd '' -)
    # json escape via python if available else sed
    if command -v python3 >/dev/null 2>&1; then
      PAYLOAD=$(python3 -c "import json,sys; print(json.dumps({'token': sys.argv[1], 'job_id': sys.argv[2], 'exit_code': int(sys.argv[3]), 'output': open('/tmp/lumi_out','r',errors='ignore').read()}))" "$TOKEN" "$JOB_ID" "$CODE")
      printf '%s' "$OUT" > /tmp/lumi_out
      PAYLOAD=$(python3 -c "import json; data={'token':open('$AGENT_DIR/agent.token').read().strip(),'job_id':'$JOB_ID','exit_code':$CODE,'output':open('/tmp/lumi_out',encoding='utf-8',errors='ignore').read()}; print(json.dumps(data))")
    else
      # fallback naive
      OUT_ESC=$(printf '%s' "$OUT" | sed ':a;N;$!ba; s/\\/\\\\/g; s/"/\\"/g; s/\n/\\n/g')
      PAYLOAD="{\"token\":\"$TOKEN\",\"job_id\":\"$JOB_ID\",\"exit_code\":$CODE,\"output\":\"$OUT_ESC\"}"
    fi
    curl -fsSL -m 10 -X POST "$URL/api/plugin/control/result" -H "Content-Type: application/json" -d "$PAYLOAD" >/dev/null 2>&1 || true
  fi
  sleep 5
done
EOSH
chmod +x "$AGENT_DIR/lumi-agent.sh"

# --- systemd --user ---
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/lumi-agent.service" <<EOSVC
[Unit]
Description=LumiAdmin LGSM Control Agent
After=network.target

[Service]
Type=simple
ExecStart=$HOME/lumi-agent/lumi-agent.sh
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
EOSVC
if command -v systemctl >/dev/null 2>&1; then
  systemctl --user daemon-reload || true
  systemctl --user enable --now lumi-agent.service || true
  echo "[lumi-agent] systemd --user lumi-agent.service enabled"
else
  echo "[lumi-agent] systemctl not found, add to crontab or run $AGENT_DIR/lumi-agent.sh manually"
  (crontab -l 2>/dev/null; echo "@reboot $HOME/lumi-agent/lumi-agent.sh >/tmp/lumi-agent.log 2>&1") | crontab - || true
fi
echo "[lumi-agent] installed. View logs: journalctl --user -u lumi-agent.service -f  or tail -f /tmp/lumi-agent.log"
"#
    .to_string()
}
