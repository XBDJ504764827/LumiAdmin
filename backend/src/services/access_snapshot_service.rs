use crate::db::Database;
use anyhow::Context;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

const SNAPSHOT_TTL_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessSnapshot {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub servers: Vec<SnapshotServer>,
    pub bans: Vec<SnapshotBan>,
    pub whitelist: Vec<SnapshotWhitelistEntry>,
    pub access_profiles: Vec<SnapshotAccessProfile>,
    pub player_access_rules: Vec<SnapshotPlayerAccessRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotServer {
    pub id: Uuid,
    pub community_id: Uuid,
    pub name: String,
    pub port: i32,
    pub report_token: String,
    pub access_restriction_enabled: bool,
    pub min_rating: i32,
    pub min_steam_level: i32,
    pub whitelist_mode_enabled: bool,
    pub use_custom_access: bool,
    pub community_whitelist_mode_enabled: bool,
    pub community_min_rating: i32,
    pub community_min_steam_level: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotBan {
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotWhitelistEntry {
    pub steam_id64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotAccessProfile {
    pub steam_id64: String,
    pub rating: i32,
    pub steam_level: i32,
    pub expires_at: DateTime<Utc>,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPlayerAccessRule {
    pub steamid64: String,
    pub allowed_servers: Vec<Uuid>,
    pub blocked_servers: Vec<Uuid>,
    pub allowed_communities: Vec<Uuid>,
    pub blocked_communities: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct SnapshotAccessInput {
    pub report_token: String,
    pub port: i32,
    pub steam_id64: String,
    pub ip_address: Option<String>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotAccessDecision {
    pub allowed: bool,
    pub message: String,
}

pub fn with_version(mut snapshot: AccessSnapshot) -> AccessSnapshot {
    snapshot.version.clear();
    let bytes = serde_json::to_vec(&snapshot).expect("snapshot serialization should not fail");
    snapshot.version = format!("{:x}", Sha256::digest(bytes));
    snapshot
}

pub async fn refresh_snapshot(
    db: &Database,
    store: &SnapshotStore,
) -> anyhow::Result<AccessSnapshot> {
    let now = Utc::now();
    let servers = load_snapshot_servers(db).await?;
    let bans = load_snapshot_bans(db).await?;
    let whitelist = load_snapshot_whitelist(db).await?;
    let access_profiles = load_snapshot_access_profiles(db).await?;
    let player_access_rules = load_snapshot_player_access_rules(db).await?;
    let snapshot = with_version(AccessSnapshot {
        version: String::new(),
        generated_at: now,
        expires_at: now + Duration::hours(SNAPSHOT_TTL_HOURS),
        servers,
        bans,
        whitelist,
        access_profiles,
        player_access_rules,
    });
    store.write_snapshot(&snapshot).await?;
    info!(version = %snapshot.version, "access snapshot refreshed");
    Ok(snapshot)
}

async fn load_snapshot_servers(db: &Database) -> anyhow::Result<Vec<SnapshotServer>> {
    sqlx::query_as::<_, SnapshotServerRow>(
        r#"SELECT s.id, s.community_id, s.name, s.port, s.report_token, s.access_restriction_enabled,
                  s.min_rating, s.min_steam_level, s.whitelist_mode_enabled,
                  s.use_custom_access, c.whitelist_mode_enabled AS community_whitelist_mode_enabled,
                  c.min_rating AS community_min_rating, c.min_steam_level AS community_min_steam_level
           FROM servers s
           JOIN communities c ON c.id = s.community_id
           WHERE s.report_token IS NOT NULL AND s.report_token <> ''"#,
    )
    .fetch_all(&db.pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .context("加载服务器访问控制快照失败")
}

async fn load_snapshot_bans(db: &Database) -> anyhow::Result<Vec<SnapshotBan>> {
    sqlx::query_as::<_, SnapshotBanRow>(
        r#"SELECT steam_id, ip_address, reason, expires_at
           FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())"#,
    )
    .fetch_all(&db.pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .context("加载封禁快照失败")
}

async fn load_snapshot_whitelist(db: &Database) -> anyhow::Result<Vec<SnapshotWhitelistEntry>> {
    sqlx::query_as::<_, SnapshotWhitelistEntryRow>(
        r#"SELECT steamid64
           FROM whitelist_requests
           WHERE status = 'approved'"#,
    )
    .fetch_all(&db.pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .context("加载白名单快照失败")
}

async fn load_snapshot_access_profiles(
    db: &Database,
) -> anyhow::Result<Vec<SnapshotAccessProfile>> {
    sqlx::query_as::<_, SnapshotAccessProfileRow>(
        r#"SELECT steamid64, rating, steam_level, expires_at
           FROM player_access_cache
           WHERE expires_at > now()"#,
    )
    .fetch_all(&db.pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .context("加载玩家访问资料快照失败")
}

#[derive(sqlx::FromRow)]
struct SnapshotPlayerAccessRuleRow {
    steamid64: String,
    allowed_servers: Vec<Uuid>,
    blocked_servers: Vec<Uuid>,
    allowed_communities: Vec<Uuid>,
    blocked_communities: Vec<Uuid>,
}

async fn load_snapshot_player_access_rules(
    db: &Database,
) -> anyhow::Result<Vec<SnapshotPlayerAccessRule>> {
    let rows: Vec<SnapshotPlayerAccessRuleRow> = sqlx::query_as(
        r#"SELECT steamid64, allowed_servers, blocked_servers, allowed_communities, blocked_communities
           FROM player_access_rules"#,
    )
    .fetch_all(&db.pool)
    .await
    .context("加载玩家进服规则快照失败")?;

    Ok(rows.into_iter().map(|r| SnapshotPlayerAccessRule {
        steamid64: r.steamid64,
        allowed_servers: r.allowed_servers,
        blocked_servers: r.blocked_servers,
        allowed_communities: r.allowed_communities,
        blocked_communities: r.blocked_communities,
    }).collect())
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotServerRow {
    id: Uuid,
    community_id: Uuid,
    name: String,
    port: i32,
    report_token: String,
    access_restriction_enabled: bool,
    min_rating: i32,
    min_steam_level: i32,
    whitelist_mode_enabled: bool,
    use_custom_access: bool,
    community_whitelist_mode_enabled: bool,
    community_min_rating: i32,
    community_min_steam_level: i32,
}

impl From<SnapshotServerRow> for SnapshotServer {
    fn from(row: SnapshotServerRow) -> Self {
        Self {
            id: row.id,
            community_id: row.community_id,
            name: row.name,
            port: row.port,
            report_token: row.report_token,
            access_restriction_enabled: row.access_restriction_enabled,
            min_rating: row.min_rating,
            min_steam_level: row.min_steam_level,
            whitelist_mode_enabled: row.whitelist_mode_enabled,
            use_custom_access: row.use_custom_access,
            community_whitelist_mode_enabled: row.community_whitelist_mode_enabled,
            community_min_rating: row.community_min_rating,
            community_min_steam_level: row.community_min_steam_level,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotBanRow {
    steam_id: Option<String>,
    ip_address: Option<String>,
    reason: String,
    expires_at: Option<DateTime<Utc>>,
}

impl From<SnapshotBanRow> for SnapshotBan {
    fn from(row: SnapshotBanRow) -> Self {
        Self {
            steam_id: row.steam_id,
            ip_address: row.ip_address,
            reason: row.reason,
            expires_at: row.expires_at,
            expires_at_unix: row.expires_at.map(|value| value.timestamp()).unwrap_or(0),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotWhitelistEntryRow {
    steamid64: String,
}

impl From<SnapshotWhitelistEntryRow> for SnapshotWhitelistEntry {
    fn from(row: SnapshotWhitelistEntryRow) -> Self {
        Self {
            steam_id64: row.steamid64,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotAccessProfileRow {
    steamid64: String,
    rating: i32,
    steam_level: i32,
    expires_at: DateTime<Utc>,
}

impl From<SnapshotAccessProfileRow> for SnapshotAccessProfile {
    fn from(row: SnapshotAccessProfileRow) -> Self {
        Self {
            steam_id64: row.steamid64,
            rating: row.rating,
            steam_level: row.steam_level,
            expires_at: row.expires_at,
            expires_at_unix: row.expires_at.timestamp(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotStore {
    path: PathBuf,
    current: Arc<RwLock<Option<AccessSnapshot>>>,
}

impl SnapshotStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            current: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn read_snapshot(&self) -> anyhow::Result<Option<AccessSnapshot>> {
        if let Some(snapshot) = self.current.read().await.clone() {
            return Ok(Some(snapshot));
        }
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = tokio::fs::read(&self.path)
            .await
            .with_context(|| format!("读取访问控制快照失败: {}", self.path.display()))?;
        let snapshot =
            serde_json::from_slice::<AccessSnapshot>(&bytes).context("解析访问控制快照失败")?;
        *self.current.write().await = Some(snapshot.clone());
        Ok(Some(snapshot))
    }

    pub async fn write_snapshot(&self, snapshot: &AccessSnapshot) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("创建访问控制快照目录失败: {}", parent.display()))?;
        }
        let tmp_path = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(snapshot).context("序列化访问控制快照失败")?;
        tokio::fs::write(&tmp_path, bytes)
            .await
            .with_context(|| format!("写入访问控制快照临时文件失败: {}", tmp_path.display()))?;
        tokio::fs::rename(&tmp_path, &self.path)
            .await
            .with_context(|| format!("替换访问控制快照失败: {}", self.path.display()))?;
        *self.current.write().await = Some(snapshot.clone());
        Ok(())
    }
}

pub fn start_refresh_loop(db: Database, store: SnapshotStore) {
    tokio::spawn(async move {
        if let Err(error) = refresh_snapshot(&db, &store).await {
            warn!(%error, "initial access snapshot refresh failed");
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            if let Err(error) = refresh_snapshot(&db, &store).await {
                error!(%error, "access snapshot refresh failed");
            }
        }
    });
}

#[derive(Debug, Clone)]
pub struct SnapshotBanInput {
    pub report_token: String,
    pub port: i32,
    pub steam_id64: Option<String>,
    pub ip_address: Option<String>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotBanDecision {
    pub available: bool,
    pub banned: bool,
    pub reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginAccessSnapshot {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub generated_at_unix: i64,
    pub expires_at_unix: i64,
    pub server: PluginSnapshotServer,
    pub bans: Vec<SnapshotBan>,
    pub whitelist: Vec<SnapshotWhitelistEntry>,
    pub access_profiles: Vec<SnapshotAccessProfile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginSnapshotServer {
    pub port: i32,
    pub access_restriction_enabled: bool,
    pub min_rating: i32,
    pub min_steam_level: i32,
    pub whitelist_mode_enabled: bool,
}

pub fn snapshot_for_plugin(
    snapshot: &AccessSnapshot,
    report_token: &str,
    port: i32,
    now: DateTime<Utc>,
) -> anyhow::Result<PluginAccessSnapshot> {
    if snapshot.expires_at <= now {
        anyhow::bail!("访问控制快照已过期");
    }
    let Some(server) = snapshot
        .servers
        .iter()
        .find(|server| server.port == port && server.report_token == report_token)
    else {
        anyhow::bail!("服务器 token 或端口无效");
    };

    let eff_restriction = effective_restriction_enabled(server);
    let eff_rating = effective_min_rating(server);
    let eff_steam_level = effective_min_steam_level(server);

    Ok(PluginAccessSnapshot {
        version: snapshot.version.clone(),
        generated_at: snapshot.generated_at,
        expires_at: snapshot.expires_at,
        generated_at_unix: snapshot.generated_at.timestamp(),
        expires_at_unix: snapshot.expires_at.timestamp(),
        server: PluginSnapshotServer {
            port: server.port,
            access_restriction_enabled: eff_restriction,
            min_rating: eff_rating,
            min_steam_level: eff_steam_level,
            whitelist_mode_enabled: server.whitelist_mode_enabled,
        },
        bans: snapshot.bans.clone(),
        whitelist: snapshot.whitelist.clone(),
        access_profiles: snapshot.access_profiles.clone(),
    })
}

pub fn evaluate_ban_snapshot(
    snapshot: &AccessSnapshot,
    input: &SnapshotBanInput,
) -> SnapshotBanDecision {
    if snapshot.expires_at <= input.now {
        return SnapshotBanDecision {
            available: false,
            banned: false,
            reason: None,
            expires_at: None,
        };
    }

    let server_exists = snapshot
        .servers
        .iter()
        .any(|server| server.port == input.port && server.report_token == input.report_token);
    if !server_exists {
        return SnapshotBanDecision {
            available: false,
            banned: false,
            reason: None,
            expires_at: None,
        };
    }

    let ban = snapshot.bans.iter().find(|ban| {
        let steam_matches = input
            .steam_id64
            .as_deref()
            .is_some_and(|steam_id64| ban.steam_id.as_deref() == Some(steam_id64));
        let ip_matches = input
            .ip_address
            .as_deref()
            .is_some_and(|ip| ban.ip_address.as_deref() == Some(ip));
        let active = ban
            .expires_at
            .is_none_or(|expires_at| expires_at > input.now);
        active && (steam_matches || ip_matches)
    });

    SnapshotBanDecision {
        available: true,
        banned: ban.is_some(),
        reason: ban.map(|ban| ban.reason.clone()),
        expires_at: ban.and_then(|ban| ban.expires_at),
    }
}

pub fn evaluate_access_snapshot(
    snapshot: &AccessSnapshot,
    input: &SnapshotAccessInput,
) -> SnapshotAccessDecision {
    if snapshot.expires_at <= input.now {
        return reject("访问控制服务暂时不可用，请稍后再试。");
    }

    let Some(server) = snapshot
        .servers
        .iter()
        .find(|server| server.port == input.port && server.report_token == input.report_token)
    else {
        return reject("访问控制服务暂时不可用，请稍后再试。");
    };

    if let Some(ban) = snapshot.bans.iter().find(|ban| {
        let steam_matches = ban.steam_id.as_deref() == Some(input.steam_id64.as_str());
        let ip_matches = input
            .ip_address
            .as_deref()
            .is_some_and(|ip| ban.ip_address.as_deref() == Some(ip));
        let active = ban
            .expires_at
            .is_none_or(|expires_at| expires_at > input.now);
        active && (steam_matches || ip_matches)
    }) {
        return reject(&format!("你已被封禁：{}", ban.reason));
    }

    // 检查玩家进服规则
    if let Some(rule) = snapshot.player_access_rules.iter().find(|r| r.steamid64 == input.steam_id64) {
        if rule.allowed_servers.contains(&server.id) {
            return allow("允许进入服务器。");
        }
        if rule.blocked_servers.contains(&server.id) {
            return reject("您被禁止进入该服务器");
        }
        if rule.allowed_communities.contains(&server.community_id) {
            return allow("允许进入服务器。");
        }
        if rule.blocked_communities.contains(&server.community_id) {
            return reject("您被禁止进入该社区的所有服务器");
        }
    }

    let has_restriction = effective_restriction_enabled(server);
    let has_whitelist = effective_whitelist_mode_enabled(server);

    // 都没开 → 无限制放行
    if !has_whitelist && !has_restriction {
        return allow("允许进入服务器。");
    }

    let whitelist_approved = snapshot
        .whitelist
        .iter()
        .any(|entry| entry.steam_id64 == input.steam_id64);

    // 仅白名单模式：必须通过白名单才能进
    if has_whitelist && !has_restriction {
        return if whitelist_approved {
            allow("已通过白名单审核，允许进入服务器。")
        } else {
            reject("你的白名单状态无法确认，请稍后再试。")
        };
    }

    // 仅进入限制：检查 rating/steam level
    if !has_whitelist && has_restriction {
        let Some(profile) = snapshot.access_profiles.iter().find(|p| {
            p.steam_id64 == input.steam_id64 && p.expires_at > input.now
        }) else {
            return reject("你的资料未满足服务器进入要求。");
        };
        if profile.rating < effective_min_rating(server) || profile.steam_level < effective_min_steam_level(server) {
            return reject("你的资料未满足服务器进入要求。");
        }
        return allow("已满足服务器进入限制，允许进入服务器。");
    }

    // 两者都开：满足限制即可进，不满足则看白名单
    if let Some(profile) = snapshot.access_profiles.iter().find(|p| {
        p.steam_id64 == input.steam_id64 && p.expires_at > input.now
    }) {
        if profile.rating >= effective_min_rating(server) && profile.steam_level >= effective_min_steam_level(server) {
            return allow("已满足服务器进入限制，允许进入服务器。");
        }
    }

    // 不满足限制，检查白名单
    if whitelist_approved {
        allow("已通过白名单审核，允许进入服务器。")
    } else {
        reject("你的资料未满足服务器进入要求。")
    }
}

fn effective_min_rating(server: &SnapshotServer) -> i32 {
    if server.use_custom_access {
        server.min_rating
    } else {
        server.community_min_rating
    }
}

fn effective_min_steam_level(server: &SnapshotServer) -> i32 {
    if server.use_custom_access {
        server.min_steam_level
    } else {
        server.community_min_steam_level
    }
}

fn effective_restriction_enabled(server: &SnapshotServer) -> bool {
    if server.use_custom_access {
        server.access_restriction_enabled
    } else {
        server.community_min_rating > 0 || server.community_min_steam_level > 0
    }
}

fn effective_whitelist_mode_enabled(server: &SnapshotServer) -> bool {
    if server.use_custom_access {
        server.whitelist_mode_enabled
    } else {
        server.community_whitelist_mode_enabled
    }
}

fn allow(message: &str) -> SnapshotAccessDecision {
    SnapshotAccessDecision {
        allowed: true,
        message: message.to_string(),
    }
}

fn reject(message: &str) -> SnapshotAccessDecision {
    SnapshotAccessDecision {
        allowed: false,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_snapshot(now: DateTime<Utc>) -> AccessSnapshot {
        AccessSnapshot {
            version: "v1".to_string(),
            generated_at: now,
            expires_at: now + Duration::hours(SNAPSHOT_TTL_HOURS),
            servers: vec![SnapshotServer {
                id: Uuid::new_v4(),
                community_id: Uuid::new_v4(),
                name: "测试服".to_string(),
                port: 27015,
                report_token: "token".to_string(),
                access_restriction_enabled: false,
                min_rating: 0,
                min_steam_level: 0,
                whitelist_mode_enabled: true,
                use_custom_access: true,
                community_whitelist_mode_enabled: false,
                community_min_rating: 0,
                community_min_steam_level: 0,
            }],
            bans: Vec::new(),
            whitelist: vec![SnapshotWhitelistEntry {
                steam_id64: "76561198000000001".to_string(),
            }],
            access_profiles: Vec::new(),
            player_access_rules: Vec::new(),
        }
    }

    fn input(now: DateTime<Utc>, steam_id64: &str) -> SnapshotAccessInput {
        SnapshotAccessInput {
            report_token: "token".to_string(),
            port: 27015,
            steam_id64: steam_id64.to_string(),
            ip_address: Some("203.0.113.10".to_string()),
            now,
        }
    }

    #[test]
    fn snapshot_allows_approved_whitelist_player() {
        let now = Utc::now();
        let snapshot = base_snapshot(now);
        let decision = evaluate_access_snapshot(&snapshot, &input(now, "76561198000000001"));

        assert!(decision.allowed);
        assert_eq!(decision.message, "已通过白名单审核，允许进入服务器。");
    }

    #[test]
    fn snapshot_rejects_player_missing_from_whitelist() {
        let now = Utc::now();
        let snapshot = base_snapshot(now);
        let decision = evaluate_access_snapshot(&snapshot, &input(now, "76561198000000002"));

        assert!(!decision.allowed);
        assert_eq!(decision.message, "你的白名单状态无法确认，请稍后再试。");
    }

    #[test]
    fn snapshot_rejects_when_expired() {
        let now = Utc::now();
        let mut snapshot = base_snapshot(now - Duration::hours(25));
        snapshot.expires_at = now - Duration::minutes(1);
        let decision = evaluate_access_snapshot(&snapshot, &input(now, "76561198000000001"));

        assert!(!decision.allowed);
        assert_eq!(decision.message, "访问控制服务暂时不可用，请稍后再试。");
    }

    #[test]
    fn snapshot_rejects_banned_player_before_whitelist() {
        let now = Utc::now();
        let mut snapshot = base_snapshot(now);
        snapshot.bans.push(SnapshotBan {
            steam_id: Some("76561198000000001".to_string()),
            ip_address: None,
            reason: "违规".to_string(),
            expires_at: None,
            expires_at_unix: 0,
        });
        let decision = evaluate_access_snapshot(&snapshot, &input(now, "76561198000000001"));

        assert!(!decision.allowed);
        assert_eq!(decision.message, "你已被封禁：违规");
    }

    #[tokio::test]
    async fn snapshot_store_keeps_previous_snapshot_when_write_input_is_valid() {
        let dir = std::env::temp_dir().join(format!("manger-snapshot-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("access_snapshot.json");
        let store = SnapshotStore::new(path.clone());
        let now = Utc::now();
        let snapshot = base_snapshot(now);

        store.write_snapshot(&snapshot).await.unwrap();
        let loaded = store.read_snapshot().await.unwrap().unwrap();

        assert_eq!(loaded.version, "v1");
        assert_eq!(loaded.servers.len(), 1);
        assert!(path.exists());

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn snapshot_version_changes_when_content_changes() {
        let now = Utc::now();
        let first = with_version(base_snapshot(now));
        let mut second_input = base_snapshot(now);
        second_input.whitelist.push(SnapshotWhitelistEntry {
            steam_id64: "76561198000000002".to_string(),
        });
        let second = with_version(second_input);

        assert_ne!(first.version, second.version);
    }

    #[tokio::test]
    async fn snapshot_store_returns_none_when_file_missing() {
        let path = std::env::temp_dir()
            .join(format!("manger-missing-snapshot-{}", Uuid::new_v4()))
            .join("access_snapshot.json");
        let store = SnapshotStore::new(path);

        assert!(store.read_snapshot().await.unwrap().is_none());
    }

    #[test]
    fn snapshot_ban_check_finds_active_ban() {
        let now = Utc::now();
        let mut snapshot = base_snapshot(now);
        snapshot.bans.push(SnapshotBan {
            steam_id: Some("76561198000000003".to_string()),
            ip_address: None,
            reason: "作弊".to_string(),
            expires_at: Some(now + Duration::hours(1)),
            expires_at_unix: (now + Duration::hours(1)).timestamp(),
        });

        let result = evaluate_ban_snapshot(
            &snapshot,
            &SnapshotBanInput {
                report_token: "token".to_string(),
                port: 27015,
                steam_id64: Some("76561198000000003".to_string()),
                ip_address: None,
                now,
            },
        );

        assert!(result.available);
        assert!(result.banned);
        assert_eq!(result.reason.as_deref(), Some("作弊"));
    }

    #[test]
    fn snapshot_ban_check_reports_unavailable_when_expired() {
        let now = Utc::now();
        let mut snapshot = base_snapshot(now - Duration::hours(25));
        snapshot.expires_at = now - Duration::minutes(1);

        let result = evaluate_ban_snapshot(
            &snapshot,
            &SnapshotBanInput {
                report_token: "token".to_string(),
                port: 27015,
                steam_id64: Some("76561198000000003".to_string()),
                ip_address: None,
                now,
            },
        );

        assert!(!result.available);
        assert!(!result.banned);
    }

    #[test]
    fn plugin_snapshot_filters_to_requested_server() {
        let now = Utc::now();
        let mut snapshot = base_snapshot(now);
        snapshot.servers.push(SnapshotServer {
            id: Uuid::new_v4(),
            community_id: Uuid::new_v4(),
            name: "其他服".to_string(),
            port: 27016,
            report_token: "other-token".to_string(),
            access_restriction_enabled: false,
            min_rating: 0,
            min_steam_level: 0,
            whitelist_mode_enabled: false,
            use_custom_access: true,
            community_whitelist_mode_enabled: false,
            community_min_rating: 0,
            community_min_steam_level: 0,
        });

        let plugin_snapshot = snapshot_for_plugin(&snapshot, "token", 27015, now).unwrap();

        assert_eq!(plugin_snapshot.server.port, 27015);
        assert_eq!(plugin_snapshot.whitelist.len(), 1);
        assert_eq!(plugin_snapshot.expires_at, snapshot.expires_at);
    }
}
