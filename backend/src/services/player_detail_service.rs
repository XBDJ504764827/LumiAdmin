use crate::{
    db::Database,
    services::{
        map_feedback_service::{query_feedback_by_steam_id, MapFeedbackItem},
        player_risk_service::{self, PlayerRiskProfile},
        r2_storage::R2Storage,
        steam_service::{ParsedSteamIdentity, SteamResolver},
    },
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct PlayerDetail {
    pub profile: PlayerProfile,
    pub summary: PlayerSummary,
    pub risk_profile: PlayerRiskProfile,
    pub whitelist: Vec<WhitelistRecord>,
    pub bans: Vec<PlayerBanRecord>,
    pub appeals: Vec<PlayerAppealRecord>,
    pub reports: Vec<PlayerReportRecord>,
    pub online_records: Vec<OnlineRecord>,
    pub player_sessions: Vec<PlayerServerSession>,
    pub access_logs: Vec<PlayerAccessRecord>,
    pub ip_history: Vec<IpHistoryEntry>,
    pub admin_actions: Vec<AdminAction>,
    pub audit_logs: Vec<PlayerAuditLog>,
    pub evidence_files: Vec<EvidenceFile>,
    pub map_feedback: Vec<MapFeedbackItem>,
    pub internal_profile: Option<PlayerInternalProfile>,
    pub timeline: Vec<TimelineEvent>,
}

#[derive(Debug, Serialize)]
pub struct PlayerProfile {
    pub steamid64: String,
    pub steamid: Option<String>,
    pub steamid3: Option<String>,
    pub profile_url: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerSearchCandidate {
    pub steamid64: String,
    pub steamid: Option<String>,
    pub steamid3: Option<String>,
    pub profile_url: Option<String>,
    pub display_name: Option<String>,
    pub sources: Vec<String>,
    pub whitelist_status: Option<String>,
    pub active_ban_count: i64,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PlayerSummary {
    pub whitelist_status: Option<String>,
    pub active_ban_count: usize,
    pub ban_count: usize,
    pub appeal_count: usize,
    pub report_count: usize,
    pub online_server_count: usize,
    pub current_online_count: usize,
    pub access_log_count: usize,
    pub access_success_count: usize,
    pub access_failure_count: usize,
    pub unique_ip_count: usize,
    pub linked_account_count: usize,
    pub linked_banned_account_count: usize,
    pub linked_global_banned_account_count: usize,
    pub evidence_file_count: usize,
    pub admin_action_count: usize,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerInternalProfile {
    pub steamid64: String,
    pub note: String,
    pub tags: Vec<String>,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WhitelistRecord {
    pub id: Uuid,
    pub steamid64: String,
    pub steamid: Option<String>,
    pub steamid3: Option<String>,
    pub profile_url: Option<String>,
    pub contact: Option<String>,
    pub nickname: String,
    pub steam_persona_name: Option<String>,
    pub status: String,
    pub source: Option<String>,
    pub applied_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub approval_reason: Option<String>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejected_by: Option<String>,
    pub rejection_reason: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerBanRecord {
    pub id: Uuid,
    pub player: Option<String>,
    pub steam_id: String,
    pub ip_address: Option<String>,
    pub server_name: Option<String>,
    pub ban_type: String,
    pub duration_minutes: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub reason: String,
    pub status: String,
    pub operator_name: String,
    pub source: String,
    pub server_id: Option<Uuid>,
    pub server_port: Option<i32>,
    pub removed_reason: Option<String>,
    pub removed_by: Option<String>,
    pub removed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerAppealRecord {
    pub id: Uuid,
    pub ban_id: Uuid,
    pub steam_id: String,
    pub player_name: String,
    pub appeal_reason: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub ban_reason: Option<String>,
    pub ban_type: Option<String>,
    pub ban_operator_name: Option<String>,
    pub ban_server_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerReportRecord {
    pub id: Uuid,
    pub target_steam_id: String,
    pub target_player_name: Option<String>,
    pub reporter_contact: Option<String>,
    pub report_reason: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct OnlineRecord {
    pub server_id: Uuid,
    pub server_name: String,
    pub community_name: Option<String>,
    pub name: String,
    pub steam_id64: String,
    pub ip: String,
    pub ping: i32,
    pub server_port: i32,
    pub current_map: String,
    pub reported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerServerSession {
    pub id: Uuid,
    pub server_id: Uuid,
    pub server_name: String,
    pub community_id: Uuid,
    pub community_name: Option<String>,
    pub player_name: Option<String>,
    pub steam_id64: String,
    pub ip: String,
    pub server_port: i32,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
    pub end_detail: Option<String>,
    pub last_ping: Option<i32>,
    pub last_map: String,
    pub duration_seconds: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerAccessRecord {
    pub id: Uuid,
    pub steam_id64: String,
    pub player_name: Option<String>,
    pub ip_address: Option<String>,
    pub server_id: Uuid,
    pub server_name: String,
    pub server_port: i32,
    pub community_id: Uuid,
    pub community_name: Option<String>,
    pub allowed: bool,
    pub access_method: String,
    pub failure_code: Option<String>,
    pub reject_reason: Option<String>,
    pub rating: Option<i32>,
    pub steam_level: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// IP 登录历史条目（玩家使用某 IP 登录的服务器列表 + 该 IP 关联的其他账号）
#[derive(Debug, Clone, Serialize)]
pub struct IpHistoryEntry {
    /// IP 地址
    pub ip: String,
    /// 该玩家使用此 IP 登录的服务器列表
    pub servers: Vec<IpServerRecord>,
    /// 该玩家使用此 IP 的最早时间
    pub first_seen: Option<DateTime<Utc>>,
    /// 该玩家使用此 IP 的最近时间
    pub last_seen: Option<DateTime<Utc>>,
    /// 使用过同一 IP 的其他账号（不同 steam_id64）
    pub linked_accounts: Vec<LinkedAccount>,
}

/// 某个 IP 在某台服务器上的登录记录
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IpServerRecord {
    pub server_name: String,
    pub server_port: Option<i32>,
    pub player_name: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub source: String,
}

/// 使用同一 IP 的关联账号（含封禁状态和登录服务器）
#[derive(Debug, Clone, Serialize)]
pub struct LinkedAccount {
    pub steam_id64: String,
    pub player_name: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    /// 是否有当前系统中的活跃封禁
    pub has_local_ban: bool,
    /// 是否有当前同步缓存中的活跃全球封禁
    pub has_global_ban: bool,
    /// 该关联账号最近一条白名单申请状态
    pub whitelist_status: Option<String>,
    /// 该关联账号白名单申请总数
    pub whitelist_count: i64,
    /// 最近一条白名单申请时间
    pub whitelist_applied_at: Option<DateTime<Utc>>,
    /// 最近一条白名单审核/撤销时间
    pub whitelist_reviewed_at: Option<DateTime<Utc>>,
    /// 最近一条白名单审核/撤销操作人
    pub whitelist_reviewer: Option<String>,
    /// 该账号登录过的服务器列表
    pub servers: Vec<String>,
    /// 使用该 IP 的次数（近似）
    pub access_count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AdminAction {
    pub operator_name: String,
    pub module: String,
    pub action: String,
    pub target_detail: String,
    pub ip_address: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerAuditLog {
    pub id: Uuid,
    pub operation: String,
    pub target: String,
    pub target_type: String,
    pub player_name: Option<String>,
    pub reason: Option<String>,
    pub duration_minutes: Option<i32>,
    pub operator_name: String,
    pub operator_steamid: Option<String>,
    pub source: String,
    pub server_id: Option<Uuid>,
    pub server_name: Option<String>,
    pub server_port: Option<i32>,
    pub success: bool,
    pub message: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceFile {
    pub source_type: String,
    pub source_id: Uuid,
    pub source_label: String,
    pub related_title: String,
    pub id: Uuid,
    pub file_name: String,
    pub file_size: i64,
    pub content_type: String,
    pub category: String,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub uploaded_by: Option<String>,
    pub uploaded_at: DateTime<Utc>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EvidenceFileRow {
    source_id: Uuid,
    id: Uuid,
    file_name: String,
    file_size: i64,
    content_type: String,
    storage_key: String,
    category: String,
    tags: Vec<String>,
    note: Option<String>,
    uploaded_by: Option<String>,
    uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PlayerInternalProfileInput {
    pub note: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EvidenceMetadataInput {
    pub note: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub event_type: String,
    pub category: String,
    pub title: String,
    pub description: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub actor: Option<String>,
    pub status: Option<String>,
    pub source: Option<String>,
    pub related_id: Option<Uuid>,
}

pub async fn get_player_detail(
    db: &Database,
    resolver: &SteamResolver,
    r2: Option<&R2Storage>,
    steam_input: &str,
) -> anyhow::Result<PlayerDetail> {
    let steamid64 = resolve_player_input(db, resolver, steam_input).await?;
    let identity = resolver.resolve(&steamid64).await?;

    // 并行查询互不依赖的数据
    let (
        whitelists,
        bans,
        appeals,
        reports,
        online_records,
        player_sessions,
        access_logs,
        ip_history,
        map_feedback,
        internal_profile,
        risk_profile,
    ) = tokio::try_join!(
        fetch_whitelist_records(db, &steamid64),
        fetch_ban_records(db, &steamid64),
        fetch_appeal_records(db, &steamid64),
        fetch_report_records(db, &steamid64),
        fetch_online_records(db, &steamid64),
        fetch_player_sessions(db, &steamid64),
        fetch_access_records(db, &steamid64),
        fetch_ip_history(db, &steamid64),
        async {
            Ok::<_, anyhow::Error>(
                query_feedback_by_steam_id(db, &steamid64)
                    .await
                    .unwrap_or_default(),
            )
        },
        fetch_internal_profile(db, &steamid64),
        player_risk_service::build_player_risk_profile(db, &steamid64),
    )?;
    // evidence_files 依赖 bans/appeals/reports，需在第一批完成后执行
    let evidence_files = fetch_evidence_files(db, r2, &bans, &appeals, &reports).await?;

    let search_terms = build_search_terms(SearchTermSources {
        identity: &identity,
        whitelists: &whitelists,
        bans: &bans,
        appeals: &appeals,
        reports: &reports,
        online_records: &online_records,
        player_sessions: &player_sessions,
        access_logs: &access_logs,
    });
    let (admin_actions, audit_logs) = tokio::try_join!(
        fetch_admin_actions(db, &search_terms),
        fetch_audit_logs(db, &search_terms),
    )?;

    let display_name = display_name(
        &whitelists,
        &bans,
        &appeals,
        &reports,
        &online_records,
        &player_sessions,
    );
    let profile = PlayerProfile {
        steamid64: steamid64.clone(),
        steamid: identity.steamid,
        steamid3: identity.steamid3,
        profile_url: identity.profile_url,
        display_name,
    };
    let linked_ids: HashSet<&str> = ip_history
        .iter()
        .flat_map(|entry| {
            entry
                .linked_accounts
                .iter()
                .map(|account| account.steam_id64.as_str())
        })
        .collect();
    let linked_banned_ids: HashSet<&str> = ip_history
        .iter()
        .flat_map(|entry| {
            entry
                .linked_accounts
                .iter()
                .filter(|account| account.has_local_ban)
                .map(|account| account.steam_id64.as_str())
        })
        .collect();
    let linked_global_banned_ids: HashSet<&str> = ip_history
        .iter()
        .flat_map(|entry| {
            entry
                .linked_accounts
                .iter()
                .filter(|account| account.has_global_ban)
                .map(|account| account.steam_id64.as_str())
        })
        .collect();
    let last_seen_at = [
        online_records.first().map(|item| item.reported_at),
        player_sessions
            .first()
            .map(|item| item.left_at.unwrap_or(item.last_seen_at)),
        access_logs.first().map(|item| item.created_at),
    ]
    .into_iter()
    .flatten()
    .max();
    let summary = PlayerSummary {
        whitelist_status: whitelists.first().map(|item| item.status.clone()),
        active_ban_count: bans.iter().filter(|ban| is_active_ban(ban)).count(),
        ban_count: bans.len(),
        appeal_count: appeals.len(),
        report_count: reports.len(),
        online_server_count: online_records.len(),
        current_online_count: online_records.len(),
        access_log_count: access_logs.len(),
        access_success_count: access_logs.iter().filter(|item| item.allowed).count(),
        access_failure_count: access_logs.iter().filter(|item| !item.allowed).count(),
        unique_ip_count: ip_history.len(),
        linked_account_count: linked_ids.len(),
        linked_banned_account_count: linked_banned_ids.len(),
        linked_global_banned_account_count: linked_global_banned_ids.len(),
        evidence_file_count: evidence_files.len(),
        admin_action_count: admin_actions.len() + audit_logs.len(),
        last_seen_at,
    };

    let timeline = build_timeline(
        &whitelists,
        &bans,
        &appeals,
        &reports,
        &online_records,
        &player_sessions,
        &access_logs,
        &admin_actions,
        &audit_logs,
        &evidence_files,
        &map_feedback,
    );

    Ok(PlayerDetail {
        profile,
        summary,
        risk_profile,
        whitelist: whitelists,
        bans,
        appeals,
        reports,
        online_records,
        player_sessions,
        access_logs,
        ip_history,
        admin_actions,
        audit_logs,
        evidence_files,
        map_feedback,
        internal_profile,
        timeline,
    })
}

pub async fn search_player_candidates(
    db: &Database,
    resolver: &SteamResolver,
    query: &str,
) -> anyhow::Result<Vec<PlayerSearchCandidate>> {
    let query = query.trim();
    if query.len() < 2 {
        return Ok(Vec::new());
    }

    let exact_identity = resolver.parse_local(query).ok();
    let exact_steamid64 = exact_identity
        .as_ref()
        .map(|identity| identity.steamid64.clone())
        .unwrap_or_default();
    let pattern = like_pattern(query);

    let rows = fetch_player_candidate_rows(db, &pattern, &exact_steamid64).await?;
    let mut candidates: HashMap<String, PlayerSearchCandidateBuilder> = HashMap::new();

    if let Some(identity) = exact_identity {
        candidates.insert(
            identity.steamid64.clone(),
            PlayerSearchCandidateBuilder::from_exact_identity(identity),
        );
    }

    for row in rows {
        let Ok(identity) = resolver.parse_local(&row.steamid64) else {
            continue;
        };
        candidates
            .entry(identity.steamid64.clone())
            .or_insert_with(|| PlayerSearchCandidateBuilder::from_identity(identity))
            .merge(row);
    }

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let steamids: Vec<String> = candidates.keys().cloned().collect();
    let active_bans = fetch_active_ban_counts(db, &steamids).await?;
    for (steamid64, count) in active_bans {
        if let Some(candidate) = candidates.get_mut(&steamid64) {
            candidate.active_ban_count = count;
        }
    }

    let mut items: Vec<PlayerSearchCandidate> = candidates
        .into_values()
        .map(PlayerSearchCandidateBuilder::finish)
        .collect();
    items.sort_by(|a, b| {
        let a_exact = !exact_steamid64.is_empty() && a.steamid64 == exact_steamid64;
        let b_exact = !exact_steamid64.is_empty() && b.steamid64 == exact_steamid64;
        b_exact
            .cmp(&a_exact)
            .then_with(|| b.active_ban_count.cmp(&a.active_ban_count))
            .then_with(|| b.sources.len().cmp(&a.sources.len()))
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
            .then_with(|| a.display_name.cmp(&b.display_name))
            .then_with(|| a.steamid64.cmp(&b.steamid64))
    });
    items.truncate(80);
    Ok(items)
}

/// 解析玩家输入：支持 SteamID64/2/3、Steam 主页链接、玩家名称、IP 地址
/// 返回 SteamID64
async fn resolve_player_input(
    db: &Database,
    resolver: &SteamResolver,
    input: &str,
) -> anyhow::Result<String> {
    let trimmed = input.trim();
    anyhow::ensure!(!trimmed.is_empty(), "查询内容不能为空");

    // 1) 先尝试作为 SteamID / 主页链接解析
    if let Ok(identity) = resolver.resolve(trimmed).await {
        return Ok(identity.steamid64);
    }

    // 2) 尝试作为 IP 地址查询
    if is_likely_ip(trimmed) {
        if let Some(steam_id) = lookup_steamid_by_ip(db, trimmed).await? {
            return Ok(steam_id);
        }
    }

    // 3) 尝试作为玩家名称查询
    if let Some(steam_id) = lookup_steamid_by_name(db, trimmed).await? {
        return Ok(steam_id);
    }

    anyhow::bail!("未找到匹配的玩家，请检查输入的 SteamID、IP 或玩家名称是否正确")
}

fn is_likely_ip(input: &str) -> bool {
    let parts: Vec<&str> = input.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

async fn lookup_steamid_by_ip(db: &Database, ip: &str) -> anyhow::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        r#"SELECT steam_id64 FROM (
            SELECT steam_id64 FROM player_access_logs WHERE ip_address = $1
            UNION
            SELECT steam_id64 FROM server_online_players WHERE ip = $1
            UNION
            SELECT steam_id64 FROM player_server_sessions WHERE ip = $1
            UNION
            SELECT steam_id AS steam_id64 FROM ban_records WHERE ip_address = $1
        ) AS ids LIMIT 1"#,
    )
    .bind(ip)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(s,)| s))
}

async fn lookup_steamid_by_name(db: &Database, name: &str) -> anyhow::Result<Option<String>> {
    let pattern = format!("%{}%", name.replace('%', "\\%").replace('_', "\\_"));
    let row: Option<(String,)> = sqlx::query_as(
        r#"SELECT steam_id64 FROM (
            SELECT steamid64 AS steam_id64 FROM whitelist_requests WHERE nickname ILIKE $1 OR steam_persona_name ILIKE $1
            UNION
            SELECT steam_id AS steam_id64 FROM ban_records WHERE player ILIKE $1
            UNION
            SELECT steam_id64 FROM player_access_logs WHERE player_name ILIKE $1
            UNION
            SELECT steam_id64 FROM server_online_players WHERE name ILIKE $1
            UNION
            SELECT steam_id64 FROM player_server_sessions WHERE player_name ILIKE $1
        ) AS ids LIMIT 1"#,
    )
    .bind(&pattern)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(s,)| s))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PlayerSearchCandidateRow {
    steamid64: String,
    display_name: Option<String>,
    source: String,
    whitelist_status: Option<String>,
    last_seen_at: Option<DateTime<Utc>>,
}

struct PlayerSearchCandidateBuilder {
    steamid64: String,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
    display_name: Option<String>,
    sources: Vec<String>,
    source_seen: HashSet<String>,
    whitelist_status: Option<String>,
    active_ban_count: i64,
    last_seen_at: Option<DateTime<Utc>>,
}

impl PlayerSearchCandidateBuilder {
    fn from_identity(identity: ParsedSteamIdentity) -> Self {
        Self {
            steamid64: identity.steamid64,
            steamid: identity.steamid,
            steamid3: identity.steamid3,
            profile_url: identity.profile_url,
            display_name: None,
            sources: Vec::new(),
            source_seen: HashSet::new(),
            whitelist_status: None,
            active_ban_count: 0,
            last_seen_at: None,
        }
    }

    fn from_exact_identity(identity: ParsedSteamIdentity) -> Self {
        let mut candidate = Self::from_identity(identity);
        candidate.add_source("直接匹配");
        candidate
    }

    fn merge(&mut self, row: PlayerSearchCandidateRow) {
        if let Some(display_name) = normalize_candidate_name(row.display_name) {
            if self.display_name.is_none() || matches!(row.source.as_str(), "在线" | "进服") {
                self.display_name = Some(display_name);
            }
        }
        self.add_source(&row.source);
        if self.whitelist_status.is_none() {
            self.whitelist_status = row.whitelist_status;
        }
        if let Some(last_seen_at) = row.last_seen_at {
            self.last_seen_at = Some(
                self.last_seen_at
                    .map_or(last_seen_at, |current| current.max(last_seen_at)),
            );
        }
    }

    fn add_source(&mut self, source: &str) {
        if self.source_seen.insert(source.to_string()) {
            self.sources.push(source.to_string());
        }
    }

    fn finish(self) -> PlayerSearchCandidate {
        PlayerSearchCandidate {
            steamid64: self.steamid64,
            steamid: self.steamid,
            steamid3: self.steamid3,
            profile_url: self.profile_url,
            display_name: self.display_name,
            sources: self.sources,
            whitelist_status: self.whitelist_status,
            active_ban_count: self.active_ban_count,
            last_seen_at: self.last_seen_at,
        }
    }
}

async fn fetch_player_candidate_rows(
    db: &Database,
    pattern: &str,
    exact_steamid64: &str,
) -> anyhow::Result<Vec<PlayerSearchCandidateRow>> {
    let rows = sqlx::query_as::<_, PlayerSearchCandidateRow>(
        r#"SELECT * FROM (
            SELECT wr.steamid64,
                   COALESCE(NULLIF(wr.steam_persona_name, ''), NULLIF(wr.nickname, '')) AS display_name,
                   '白名单'::TEXT AS source,
                   wr.status AS whitelist_status,
                   COALESCE(wr.updated_at, wr.approved_at, wr.rejected_at, wr.revoked_at, wr.applied_at) AS last_seen_at
            FROM whitelist_requests wr
            WHERE wr.steamid64 IS NOT NULL
              AND btrim(wr.steamid64) <> ''
              AND (
                wr.steamid64 = NULLIF($2, '')
                OR wr.steamid64 ILIKE $1 ESCAPE '\'
                OR wr.steamid ILIKE $1 ESCAPE '\'
                OR wr.steamid3 ILIKE $1 ESCAPE '\'
                OR wr.nickname ILIKE $1 ESCAPE '\'
                OR wr.steam_persona_name ILIKE $1 ESCAPE '\'
              )
            ORDER BY COALESCE(wr.updated_at, wr.applied_at) DESC NULLS LAST
            LIMIT 80
        ) whitelist_matches
        UNION ALL
        SELECT * FROM (
            SELECT br.steam_id AS steamid64,
                   NULLIF(br.player, '') AS display_name,
                   '封禁'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   br.created_at AS last_seen_at
            FROM ban_records br
            WHERE br.steam_id IS NOT NULL
              AND btrim(br.steam_id) <> ''
              AND (
                br.steam_id = NULLIF($2, '')
                OR br.steam_id ILIKE $1 ESCAPE '\'
                OR br.player ILIKE $1 ESCAPE '\'
                OR br.ip_address ILIKE $1 ESCAPE '\'
              )
            ORDER BY br.created_at DESC
            LIMIT 80
        ) ban_matches
        UNION ALL
        SELECT * FROM (
            SELECT sop.steam_id64,
                   NULLIF(sop.name, '') AS display_name,
                   '在线'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   sop.reported_at AS last_seen_at
            FROM server_online_players sop
            WHERE sop.steam_id64 IS NOT NULL
              AND btrim(sop.steam_id64) <> ''
              AND (
                sop.steam_id64 = NULLIF($2, '')
                OR sop.steam_id64 ILIKE $1 ESCAPE '\'
                OR sop.name ILIKE $1 ESCAPE '\'
                OR sop.ip ILIKE $1 ESCAPE '\'
              )
            ORDER BY sop.reported_at DESC
            LIMIT 80
        ) online_matches
        UNION ALL
        SELECT * FROM (
            SELECT pss.steam_id64,
                   NULLIF(pss.player_name, '') AS display_name,
                   '会话'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   COALESCE(pss.left_at, pss.last_seen_at) AS last_seen_at
            FROM player_server_sessions pss
            WHERE pss.steam_id64 IS NOT NULL
              AND btrim(pss.steam_id64) <> ''
              AND (
                pss.steam_id64 = NULLIF($2, '')
                OR pss.steam_id64 ILIKE $1 ESCAPE '\'
                OR pss.player_name ILIKE $1 ESCAPE '\'
                OR pss.ip ILIKE $1 ESCAPE '\'
              )
            ORDER BY COALESCE(pss.left_at, pss.last_seen_at) DESC
            LIMIT 80
        ) session_matches
        UNION ALL
        SELECT * FROM (
            SELECT pal.steam_id64,
                   NULLIF(pal.player_name, '') AS display_name,
                   '进服'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   pal.created_at AS last_seen_at
            FROM player_access_logs pal
            WHERE pal.steam_id64 IS NOT NULL
              AND btrim(pal.steam_id64) <> ''
              AND (
                pal.steam_id64 = NULLIF($2, '')
                OR pal.steam_id64 ILIKE $1 ESCAPE '\'
                OR pal.player_name ILIKE $1 ESCAPE '\'
                OR pal.ip_address ILIKE $1 ESCAPE '\'
              )
            ORDER BY pal.created_at DESC
            LIMIT 80
        ) access_matches
        UNION ALL
        SELECT * FROM (
            SELECT pr.target_steam_id AS steamid64,
                   NULLIF(pr.target_player_name, '') AS display_name,
                   '举报'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   pr.created_at AS last_seen_at
            FROM player_reports pr
            WHERE pr.target_steam_id IS NOT NULL
              AND btrim(pr.target_steam_id) <> ''
              AND (
                pr.target_steam_id = NULLIF($2, '')
                OR pr.target_steam_id ILIKE $1 ESCAPE '\'
                OR pr.target_player_name ILIKE $1 ESCAPE '\'
              )
            ORDER BY pr.created_at DESC
            LIMIT 80
        ) report_matches
        UNION ALL
        SELECT * FROM (
            SELECT ba.steam_id AS steamid64,
                   NULLIF(ba.player_name, '') AS display_name,
                   '申诉'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   ba.created_at AS last_seen_at
            FROM ban_appeals ba
            WHERE ba.steam_id IS NOT NULL
              AND btrim(ba.steam_id) <> ''
              AND (
                ba.steam_id = NULLIF($2, '')
                OR ba.steam_id ILIKE $1 ESCAPE '\'
                OR ba.player_name ILIKE $1 ESCAPE '\'
              )
            ORDER BY ba.created_at DESC
            LIMIT 80
        ) appeal_matches
        UNION ALL
        SELECT * FROM (
            SELECT mf.steam_id AS steamid64,
                   NULLIF(mf.steam_persona_name, '') AS display_name,
                   '地图反馈'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   mf.created_at AS last_seen_at
            FROM map_feedback mf
            WHERE mf.steam_id IS NOT NULL
              AND btrim(mf.steam_id) <> ''
              AND (
                mf.steam_id = NULLIF($2, '')
                OR mf.steam_id ILIKE $1 ESCAPE '\'
                OR mf.steam_persona_name ILIKE $1 ESCAPE '\'
              )
            ORDER BY mf.created_at DESC
            LIMIT 80
        ) feedback_matches
        UNION ALL
        SELECT * FROM (
            SELECT gb.steam_id64,
                   NULLIF(gb.player_name, '') AS display_name,
                   '全球封禁'::TEXT AS source,
                   NULL::TEXT AS whitelist_status,
                   gb.synced_at AS last_seen_at
            FROM global_bans gb
            WHERE gb.steam_id64 IS NOT NULL
              AND btrim(gb.steam_id64) <> ''
              AND (
                gb.steam_id64 = NULLIF($2, '')
                OR gb.steam_id64 ILIKE $1 ESCAPE '\'
                OR gb.steam_id ILIKE $1 ESCAPE '\'
                OR gb.player_name ILIKE $1 ESCAPE '\'
              )
            ORDER BY gb.synced_at DESC
            LIMIT 80
        ) global_ban_matches"#,
    )
    .bind(pattern)
    .bind(exact_steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_active_ban_counts(
    db: &Database,
    steamids: &[String],
) -> anyhow::Result<Vec<(String, i64)>> {
    if steamids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (String, i64)>(
        r#"SELECT steam_id, COUNT(*)::BIGINT AS active_ban_count
           FROM ban_records
           WHERE steam_id = ANY($1)
             AND status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
           GROUP BY steam_id"#,
    )
    .bind(steamids)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

fn like_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn normalize_candidate_name(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(value)
    }
}

pub async fn upsert_player_internal_profile(
    db: &Database,
    steamid64: &str,
    input: PlayerInternalProfileInput,
    updated_by: &str,
) -> anyhow::Result<PlayerInternalProfile> {
    let steamid64 = steamid64.trim();
    anyhow::ensure!(
        steamid64.len() == 17 && steamid64.chars().all(|value| value.is_ascii_digit()),
        "SteamID64 格式无效"
    );
    let note = normalize_note(input.note, 2000)?;
    let tags = normalize_tags(input.tags)?;

    let row = sqlx::query_as::<_, PlayerInternalProfile>(
        r#"INSERT INTO player_internal_notes (steamid64, note, tags, updated_by, updated_at, created_at)
           VALUES ($1, $2, $3, $4, now(), now())
           ON CONFLICT (steamid64) DO UPDATE
           SET note = EXCLUDED.note,
               tags = EXCLUDED.tags,
               updated_by = EXCLUDED.updated_by,
               updated_at = now()
           RETURNING steamid64, note, tags, updated_by, updated_at, created_at"#,
    )
    .bind(steamid64)
    .bind(note)
    .bind(tags)
    .bind(updated_by)
    .fetch_one(&db.pool)
    .await?;
    Ok(row)
}

pub async fn update_evidence_metadata(
    db: &Database,
    source_type: &str,
    file_id: Uuid,
    input: EvidenceMetadataInput,
) -> anyhow::Result<EvidenceFile> {
    let (table_name, source_column, source_label) = evidence_table(source_type)?;
    let uploaded_by_select = if source_type == "ban" {
        "uploaded_by"
    } else {
        "NULL::TEXT AS uploaded_by"
    };
    let note = normalize_note(input.note, 1000)?;
    let tags = normalize_tags(input.tags)?;
    let sql = format!(
        r#"UPDATE {table_name}
           SET tags = $2, note = $3
           WHERE id = $1
           RETURNING {source_column} AS source_id, id, file_name, file_size, content_type,
                     storage_key, category, tags, note, {uploaded_by_select}, uploaded_at"#
    );
    let row = sqlx::query_as::<_, EvidenceFileRow>(&sql)
        .bind(file_id)
        .bind(tags)
        .bind(note)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("证据文件不存在"))?;

    Ok(map_evidence_file(
        row,
        source_type,
        source_label,
        source_label.to_string(),
        None,
    ))
}

pub async fn fetch_internal_profile(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<PlayerInternalProfile>> {
    let row = sqlx::query_as::<_, PlayerInternalProfile>(
        r#"SELECT steamid64, note, tags, updated_by, updated_at, created_at
           FROM player_internal_notes
           WHERE steamid64 = $1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row)
}

async fn fetch_whitelist_records(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<WhitelistRecord>> {
    let rows = sqlx::query_as::<_, WhitelistRecord>(
        r#"SELECT wr.id, wr.steamid64, wr.steamid, wr.steamid3, wr.profile_url, wr.contact,
                  wr.nickname, wr.steam_persona_name, wr.status, wr.source,
                  wr.applied_at,
                  wr.approved_at, COALESCE(approved_user.display_name, wr.approved_by) AS approved_by, wr.approval_reason,
                  wr.rejected_at, COALESCE(rejected_user.display_name, wr.rejected_by) AS rejected_by, wr.rejection_reason,
                  wr.revoked_at, COALESCE(revoked_user.display_name, wr.revoked_by) AS revoked_by,
                  wr.updated_at
           FROM whitelist_requests wr
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = wr.approved_by
                OR u.display_name = wr.approved_by
                OR NULLIF(u.remark, '') = wr.approved_by
             ORDER BY CASE WHEN u.username = wr.approved_by THEN 0 WHEN u.display_name = wr.approved_by THEN 1 ELSE 2 END
             LIMIT 1
           ) approved_user ON true
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = wr.rejected_by
                OR u.display_name = wr.rejected_by
                OR NULLIF(u.remark, '') = wr.rejected_by
             ORDER BY CASE WHEN u.username = wr.rejected_by THEN 0 WHEN u.display_name = wr.rejected_by THEN 1 ELSE 2 END
             LIMIT 1
           ) rejected_user ON true
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = wr.revoked_by
                OR u.display_name = wr.revoked_by
                OR NULLIF(u.remark, '') = wr.revoked_by
             ORDER BY CASE WHEN u.username = wr.revoked_by THEN 0 WHEN u.display_name = wr.revoked_by THEN 1 ELSE 2 END
             LIMIT 1
           ) revoked_user ON true
           WHERE wr.steamid64 = $1
           ORDER BY wr.updated_at DESC NULLS LAST, wr.applied_at DESC
           LIMIT 50"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_ban_records(db: &Database, steamid64: &str) -> anyhow::Result<Vec<PlayerBanRecord>> {
    let rows = sqlx::query_as::<_, PlayerBanRecord>(
        r#"SELECT br.id, br.player, br.steam_id, br.ip_address, br.server_name, br.ban_type,
                  br.duration_minutes, br.expires_at, br.reason, br.status,
                  COALESCE(operator_user.display_name, br.operator_name) AS operator_name,
                  br.source, br.server_id, br.server_port, br.removed_reason,
                  COALESCE(removed_user.display_name, br.removed_by) AS removed_by,
                  br.removed_at, br.created_at
           FROM ban_records br
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = br.operator_name
                OR u.display_name = br.operator_name
                OR NULLIF(u.remark, '') = br.operator_name
             ORDER BY CASE WHEN u.username = br.operator_name THEN 0 WHEN u.display_name = br.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) operator_user ON true
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = br.removed_by
                OR u.display_name = br.removed_by
                OR NULLIF(u.remark, '') = br.removed_by
             ORDER BY CASE WHEN u.username = br.removed_by THEN 0 WHEN u.display_name = br.removed_by THEN 1 ELSE 2 END
             LIMIT 1
           ) removed_user ON true
           WHERE br.steam_id = $1
           ORDER BY br.created_at DESC
           LIMIT 100"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_appeal_records(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<PlayerAppealRecord>> {
    let rows = sqlx::query_as::<_, PlayerAppealRecord>(
        r#"SELECT ba.id, ba.ban_id, ba.steam_id, ba.player_name, ba.appeal_reason,
                  ba.status, COALESCE(reviewer_user.display_name, ba.reviewed_by) AS reviewed_by,
                  ba.review_note, ba.reviewed_at, ba.created_at,
                  br.reason AS ban_reason, br.ban_type,
                  COALESCE(ban_operator_user.display_name, br.operator_name) AS ban_operator_name,
                  br.server_name AS ban_server_name
           FROM ban_appeals ba
           LEFT JOIN ban_records br ON br.id = ba.ban_id
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = ba.reviewed_by
                OR u.display_name = ba.reviewed_by
                OR NULLIF(u.remark, '') = ba.reviewed_by
             ORDER BY CASE WHEN u.username = ba.reviewed_by THEN 0 WHEN u.display_name = ba.reviewed_by THEN 1 ELSE 2 END
             LIMIT 1
           ) reviewer_user ON true
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = br.operator_name
                OR u.display_name = br.operator_name
                OR NULLIF(u.remark, '') = br.operator_name
             ORDER BY CASE WHEN u.username = br.operator_name THEN 0 WHEN u.display_name = br.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) ban_operator_user ON true
           WHERE ba.steam_id = $1
           ORDER BY ba.created_at DESC
           LIMIT 100"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_report_records(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<PlayerReportRecord>> {
    let rows = sqlx::query_as::<_, PlayerReportRecord>(
        r#"SELECT pr.id, pr.target_steam_id, pr.target_player_name, pr.reporter_contact,
                  pr.report_reason, pr.status, COALESCE(reviewer_user.display_name, pr.reviewed_by) AS reviewed_by,
                  pr.review_note, pr.reviewed_at, pr.created_at
           FROM player_reports pr
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = pr.reviewed_by
                OR u.display_name = pr.reviewed_by
                OR NULLIF(u.remark, '') = pr.reviewed_by
             ORDER BY CASE WHEN u.username = pr.reviewed_by THEN 0 WHEN u.display_name = pr.reviewed_by THEN 1 ELSE 2 END
             LIMIT 1
           ) reviewer_user ON true
           WHERE pr.target_steam_id = $1
           ORDER BY pr.created_at DESC
           LIMIT 100"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_online_records(db: &Database, steamid64: &str) -> anyhow::Result<Vec<OnlineRecord>> {
    let rows = sqlx::query_as::<_, OnlineRecord>(
        r#"SELECT sop.server_id, s.name AS server_name, c.name AS community_name,
                  sop.name, sop.steam_id64, sop.ip, sop.ping, sop.server_port,
                  sop.current_map, sop.reported_at
           FROM server_online_players sop
           JOIN servers s ON s.id = sop.server_id
           LEFT JOIN communities c ON c.id = s.community_id
           WHERE sop.steam_id64 = $1
           ORDER BY sop.reported_at DESC"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_player_sessions(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<PlayerServerSession>> {
    let rows = sqlx::query_as::<_, PlayerServerSession>(
        r#"SELECT id, server_id, server_name, community_id, community_name,
                  player_name, steam_id64, ip, server_port, first_seen_at,
                  last_seen_at, left_at, end_reason, end_detail, last_ping, last_map,
                  GREATEST(
                    0,
                    EXTRACT(EPOCH FROM (COALESCE(left_at, last_seen_at) - first_seen_at))::BIGINT
                  ) AS duration_seconds
           FROM player_server_sessions
           WHERE steam_id64 = $1
           ORDER BY COALESCE(left_at, last_seen_at) DESC
           LIMIT 200"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

async fn fetch_access_records(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<PlayerAccessRecord>> {
    let rows = sqlx::query_as::<_, PlayerAccessRecord>(
        r#"SELECT id, steam_id64, player_name, ip_address, server_id, server_name,
                  server_port, community_id, community_name, allowed, access_method,
                  failure_code, reject_reason, rating, steam_level, created_at
           FROM player_access_logs
           WHERE steam_id64 = $1
           ORDER BY created_at DESC
           LIMIT 200"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// 批量查询的辅助 FromRow 结构（带分组键 ip / steam_id）
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct IpServerRow {
    ip: String,
    server_name: String,
    server_port: Option<i32>,
    player_name: Option<String>,
    last_seen: Option<DateTime<Utc>>,
    source: String,
}

#[derive(sqlx::FromRow)]
struct IpTimeRow {
    ip: String,
    first_seen: Option<DateTime<Utc>>,
    last_seen: Option<DateTime<Utc>>,
}

/// (最早时间, 最近时间)
type IpTimeRange = (Option<DateTime<Utc>>, Option<DateTime<Utc>>);

#[derive(sqlx::FromRow)]
struct IpLinkedRow {
    ip: String,
    steam_id: String,
}

#[derive(sqlx::FromRow)]
struct LinkedNameRow {
    steam_id64: String,
    player_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct LinkedCountRow {
    steam_id: String,
    cnt: i64,
}

#[derive(sqlx::FromRow)]
struct LinkedWhitelistRow {
    steam_id: String,
    status: String,
    applied_at: DateTime<Utc>,
    reviewed_at: Option<DateTime<Utc>>,
    reviewer: Option<String>,
    whitelist_count: i64,
}

#[derive(sqlx::FromRow)]
struct LinkedServerRow {
    steam_id: String,
    server_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct LinkedTimeRow {
    steam_id: String,
    last_seen: Option<DateTime<Utc>>,
}

/// 查询玩家使用过的 IP 地址历史，包括：
/// - 每个 IP 登录过哪些服务器
/// - 每个 IP 关联的其他账号（同 IP 不同 SteamID64）
///
/// 数据来源：player_access_logs + server_online_players + player_server_sessions + ban_records
///
/// 优化说明：原实现对每个 IP/关联账号逐条查询（N+1），最坏情况下单次请求会发起
/// 数千次数据库查询。现改为基于 `= ANY($n)` 的批量查询，在内存中按 ip / steam_id
/// 分组聚合，总查询数从 ~6000+ 降到 ~12 次（且分两批并行）。
async fn fetch_ip_history(db: &Database, steamid64: &str) -> anyhow::Result<Vec<IpHistoryEntry>> {
    // ① 收集该玩家所有使用过的 IP（来源：player_access_logs + server_online_players + player_server_sessions + ban_records）
    //    使用 UNION 去重后获取唯一的 IP 列表
    let ip_rows: Vec<(String,)> = sqlx::query_as(
        r#"SELECT DISTINCT ip FROM (
            SELECT ip_address AS ip FROM player_access_logs
            WHERE steam_id64 = $1 AND ip_address IS NOT NULL AND btrim(ip_address) <> ''
            UNION
            SELECT ip FROM server_online_players
            WHERE steam_id64 = $1 AND ip IS NOT NULL AND btrim(ip) <> ''
            UNION
            SELECT ip FROM player_server_sessions
            WHERE steam_id64 = $1 AND ip IS NOT NULL AND btrim(ip) <> ''
            UNION
            SELECT ip_address AS ip FROM ban_records
            WHERE steam_id = $1 AND ip_address IS NOT NULL AND btrim(ip_address) <> ''
        ) AS ips
        WHERE btrim(ip) <> '' AND ip <> 'unknown'
        LIMIT 50"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await?;

    let player_ips: Vec<String> = ip_rows.into_iter().map(|(ip,)| ip).collect();
    if player_ips.is_empty() {
        return Ok(Vec::new());
    }

    // ②③④ 第一批：并行查询仅依赖 player_ips 的三组数据
    let (ip_server_rows, ip_time_rows, ip_linked_rows) = tokio::try_join!(
        fetch_ip_server_rows(db, steamid64, &player_ips),
        fetch_ip_time_rows(db, steamid64, &player_ips),
        fetch_ip_linked_rows(db, steamid64, &player_ips),
    )?;

    // 按 IP 分组：服务器列表（每个 IP 保留前 50 条，已由 SQL 窗口函数保证）
    let mut servers_by_ip: HashMap<String, Vec<IpServerRecord>> = HashMap::new();
    for row in ip_server_rows {
        servers_by_ip
            .entry(row.ip)
            .or_default()
            .push(IpServerRecord {
                server_name: row.server_name,
                server_port: row.server_port,
                player_name: row.player_name,
                last_seen: row.last_seen,
                source: row.source,
            });
    }

    // 按 IP 分组：最早/最晚时间
    let times_by_ip: HashMap<String, IpTimeRange> = ip_time_rows
        .into_iter()
        .map(|r| (r.ip, (r.first_seen, r.last_seen)))
        .collect();

    // 按 IP 分组：关联账号（每个 IP 保留前 30 个，已由 SQL 窗口函数保证）
    let mut linked_by_ip: HashMap<String, Vec<String>> = HashMap::new();
    for row in ip_linked_rows {
        linked_by_ip.entry(row.ip).or_default().push(row.steam_id);
    }

    // 收集所有关联账号并去重，用于第二批批量查询
    let all_linked_ids: Vec<String> = linked_by_ip
        .values()
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .cloned()
        .collect();

    // 若无关联账号，直接组装 entries 返回
    let account_map = if all_linked_ids.is_empty() {
        HashMap::new()
    } else {
        fetch_linked_accounts(db, &all_linked_ids).await?
    };

    // 组装每个 IP 的 IpHistoryEntry
    let mut entries = Vec::with_capacity(player_ips.len());
    for ip in &player_ips {
        let (first_seen, last_seen) = times_by_ip.get(ip).copied().unwrap_or((None, None));
        let mut linked_accounts: Vec<LinkedAccount> = linked_by_ip
            .get(ip)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| account_map.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default();
        // 关联账号按最近活跃时间降序
        #[allow(clippy::unnecessary_sort_by)]
        linked_accounts.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));

        entries.push(IpHistoryEntry {
            ip: ip.clone(),
            servers: servers_by_ip.remove(ip).unwrap_or_default(),
            first_seen,
            last_seen,
            linked_accounts,
        });
    }

    // 按 last_seen 降序排列
    #[allow(clippy::unnecessary_sort_by)]
    entries.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    Ok(entries)
}

/// ② 批量查询：该玩家在各 IP 上登录过的服务器（每个 IP 保留最近 50 条）
async fn fetch_ip_server_rows(
    db: &Database,
    steamid64: &str,
    ips: &[String],
) -> anyhow::Result<Vec<IpServerRow>> {
    sqlx::query_as::<_, IpServerRow>(
        r#"SELECT ip, server_name, server_port, player_name, last_seen, source FROM (
            SELECT ip, server_name, server_port, player_name, last_seen, source,
                   ROW_NUMBER() OVER (PARTITION BY ip ORDER BY last_seen DESC) AS rn
            FROM (
                SELECT ip_address AS ip, server_name, server_port::INTEGER AS server_port,
                       player_name, max(created_at) AS last_seen, 'access_log' AS source
                FROM player_access_logs
                WHERE steam_id64 = $1 AND ip_address = ANY($2)
                GROUP BY ip_address, server_name, server_port, player_name
                UNION ALL
                SELECT sop.ip AS ip, s.name AS server_name, sop.server_port::INTEGER AS server_port,
                       sop.name AS player_name, max(sop.reported_at) AS last_seen, 'online_report' AS source
                FROM server_online_players sop
                JOIN servers s ON s.id = sop.server_id
                WHERE sop.steam_id64 = $1 AND sop.ip = ANY($2)
                GROUP BY sop.ip, s.name, sop.server_port, sop.name
                UNION ALL
                SELECT pss.ip AS ip, pss.server_name, pss.server_port::INTEGER AS server_port,
                       pss.player_name, max(COALESCE(pss.left_at, pss.last_seen_at)) AS last_seen,
                       'server_session' AS source
                FROM player_server_sessions pss
                WHERE pss.steam_id64 = $1 AND pss.ip = ANY($2)
                GROUP BY pss.ip, pss.server_name, pss.server_port, pss.player_name
            ) AS combined
        ) AS ranked
        WHERE rn <= 50"#,
    )
    .bind(steamid64)
    .bind(ips)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

/// ③ 批量查询：该玩家在各 IP 上的最早/最晚时间
async fn fetch_ip_time_rows(
    db: &Database,
    steamid64: &str,
    ips: &[String],
) -> anyhow::Result<Vec<IpTimeRow>> {
    sqlx::query_as::<_, IpTimeRow>(
        r#"SELECT ip, min(ts) AS first_seen, max(ts) AS last_seen FROM (
            SELECT ip_address AS ip, created_at AS ts FROM player_access_logs
            WHERE steam_id64 = $1 AND ip_address = ANY($2)
            UNION ALL
            SELECT ip, reported_at AS ts FROM server_online_players
            WHERE steam_id64 = $1 AND ip = ANY($2)
            UNION ALL
            SELECT ip, first_seen_at AS ts FROM player_server_sessions
            WHERE steam_id64 = $1 AND ip = ANY($2)
            UNION ALL
            SELECT ip, COALESCE(left_at, last_seen_at) AS ts FROM player_server_sessions
            WHERE steam_id64 = $1 AND ip = ANY($2)
            UNION ALL
            SELECT ip_address AS ip, created_at AS ts FROM ban_records
            WHERE steam_id = $1 AND ip_address = ANY($2)
        ) AS times
        GROUP BY ip"#,
    )
    .bind(steamid64)
    .bind(ips)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

/// ④ 批量查询：各 IP 关联的其他账号（不同 SteamID64，每个 IP 保留前 30 个）
async fn fetch_ip_linked_rows(
    db: &Database,
    steamid64: &str,
    ips: &[String],
) -> anyhow::Result<Vec<IpLinkedRow>> {
    sqlx::query_as::<_, IpLinkedRow>(
        r#"SELECT ip, steam_id FROM (
            SELECT ip, steam_id,
                   ROW_NUMBER() OVER (PARTITION BY ip ORDER BY steam_id) AS rn
            FROM (
                SELECT ip_address AS ip, steam_id64 AS steam_id FROM player_access_logs
                WHERE steam_id64 <> $1 AND ip_address = ANY($2)
                UNION
                SELECT ip, steam_id64 AS steam_id FROM server_online_players
                WHERE steam_id64 <> $1 AND ip = ANY($2)
                UNION
                SELECT ip, steam_id64 AS steam_id FROM player_server_sessions
                WHERE steam_id64 <> $1 AND ip = ANY($2)
                UNION
                SELECT ip_address AS ip, steam_id AS steam_id FROM ban_records
                WHERE steam_id <> $1 AND ip_address = ANY($2)
            ) AS ids
        ) AS ranked
        WHERE rn <= 30"#,
    )
    .bind(steamid64)
    .bind(ips)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

/// ⑤⑥⑦⑧⑨ 第二批：批量查询所有关联账号的详细信息（玩家名/封禁/服务器/时间/次数）。
/// 返回 steam_id -> LinkedAccount 的映射，名字按优先级合并（access_log > online > ban > whitelist）。
async fn fetch_linked_accounts(
    db: &Database,
    linked_ids: &[String],
) -> anyhow::Result<HashMap<String, LinkedAccount>> {
    // ⑤ 玩家名：四个来源各自取每个账号最新的一条，再在内存中按优先级合并
    let (
        name_access,
        name_online,
        name_session,
        name_ban,
        name_wl,
        ban_rows,
        global_ban_rows,
        synced_global_ban_rows,
        whitelist_rows,
        server_rows,
        time_rows,
        count_rows,
    ) = tokio::try_join!(
        async {
            sqlx::query_as::<_, LinkedNameRow>(
                r#"SELECT DISTINCT ON (steam_id64) steam_id64, player_name
                       FROM player_access_logs
                       WHERE steam_id64 = ANY($1)
                       ORDER BY steam_id64, created_at DESC"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        async {
            sqlx::query_as::<_, LinkedNameRow>(
                r#"SELECT DISTINCT ON (steam_id64) steam_id64, name AS player_name
                       FROM server_online_players
                       WHERE steam_id64 = ANY($1)
                       ORDER BY steam_id64, reported_at DESC"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        async {
            sqlx::query_as::<_, LinkedNameRow>(
                r#"SELECT DISTINCT ON (steam_id64) steam_id64, player_name
                       FROM player_server_sessions
                       WHERE steam_id64 = ANY($1)
                       ORDER BY steam_id64, COALESCE(left_at, last_seen_at) DESC"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        async {
            sqlx::query_as::<_, LinkedNameRow>(
                r#"SELECT DISTINCT ON (steam_id) steam_id AS steam_id64, player AS player_name
                       FROM ban_records
                       WHERE steam_id = ANY($1)
                       ORDER BY steam_id, created_at DESC"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        async {
            sqlx::query_as::<_, LinkedNameRow>(
                r#"SELECT DISTINCT ON (steamid64) steamid64 AS steam_id64, nickname AS player_name
                       FROM whitelist_requests
                       WHERE steamid64 = ANY($1)
                       ORDER BY steamid64, COALESCE(updated_at, applied_at) DESC"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        // ⑥ LumiAdmin 本地活跃封禁数（不含全球封禁同步生成的本地记录）
        async {
            sqlx::query_as::<_, LinkedCountRow>(
                r#"SELECT steam_id, COUNT(*) AS cnt FROM ban_records
                       WHERE status = 'active'
                         AND steam_id = ANY($1)
                         AND source <> 'global_ban'
                       GROUP BY steam_id"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        // ⑥.1 活跃全球封禁数
        async {
            sqlx::query_as::<_, LinkedCountRow>(
                r#"SELECT steam_id64 AS steam_id, COUNT(*) AS cnt FROM global_bans
                       WHERE steam_id64 = ANY($1)
                         AND is_expired = false
                         AND manual_unbanned = false
                       GROUP BY steam_id64"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        // ⑥.2 由全球封禁同步生成的本地封禁数
        async {
            sqlx::query_as::<_, LinkedCountRow>(
                r#"SELECT steam_id, COUNT(*) AS cnt FROM ban_records
                       WHERE steam_id = ANY($1)
                         AND status = 'active'
                         AND source = 'global_ban'
                       GROUP BY steam_id"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        // ⑦ 最近白名单状态与申请次数
        async {
            sqlx::query_as::<_, LinkedWhitelistRow>(
                    r#"SELECT steam_id, status, applied_at, reviewed_at, reviewer, whitelist_count FROM (
                        SELECT steamid64 AS steam_id, status, applied_at,
                               COALESCE(approved_at, rejected_at, revoked_at) AS reviewed_at,
                               COALESCE(approved_by, rejected_by, revoked_by) AS reviewer,
                               COUNT(*) OVER (PARTITION BY steamid64) AS whitelist_count,
                               ROW_NUMBER() OVER (
                                 PARTITION BY steamid64
                                 ORDER BY COALESCE(updated_at, approved_at, rejected_at, revoked_at, applied_at) DESC
                               ) AS rn
                        FROM whitelist_requests
                        WHERE steamid64 = ANY($1)
                    ) AS ranked
                    WHERE rn = 1"#,
                )
                .bind(linked_ids)
                .fetch_all(&db.pool)
                .await
                .map_err(anyhow::Error::from)
        },
        // ⑧ 登录过的服务器（每个账号前 20 个）
        async {
            sqlx::query_as::<_, LinkedServerRow>(
                r#"SELECT steam_id, server_name FROM (
                        SELECT steam_id, server_name,
                               ROW_NUMBER() OVER (PARTITION BY steam_id ORDER BY server_name) AS rn
                        FROM (
                            SELECT steam_id64 AS steam_id, server_name FROM player_access_logs
                            WHERE steam_id64 = ANY($1) AND server_name IS NOT NULL
                            UNION
                            SELECT sop.steam_id64 AS steam_id, s.name AS server_name
                            FROM server_online_players sop
                            JOIN servers s ON s.id = sop.server_id
                            WHERE sop.steam_id64 = ANY($1)
                            UNION
                            SELECT steam_id64 AS steam_id, server_name
                            FROM player_server_sessions
                            WHERE steam_id64 = ANY($1)
                        ) AS servers
                    ) AS ranked
                    WHERE rn <= 20"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        // ⑨ 最近活跃时间
        async {
            sqlx::query_as::<_, LinkedTimeRow>(
                r#"SELECT steam_id, max(ts) AS last_seen FROM (
                        SELECT steam_id64 AS steam_id, created_at AS ts FROM player_access_logs
                        WHERE steam_id64 = ANY($1)
                        UNION ALL
                        SELECT steam_id64 AS steam_id, reported_at AS ts FROM server_online_players
                        WHERE steam_id64 = ANY($1)
                        UNION ALL
                        SELECT steam_id64 AS steam_id, COALESCE(left_at, last_seen_at) AS ts FROM player_server_sessions
                        WHERE steam_id64 = ANY($1)
                        UNION ALL
                        SELECT steam_id AS steam_id, created_at AS ts FROM ban_records
                        WHERE steam_id = ANY($1)
                    ) AS times
                    GROUP BY steam_id"#,
            )
            .bind(linked_ids)
            .fetch_all(&db.pool)
            .await
            .map_err(anyhow::Error::from)
        },
        // ⑩ 访问次数（player_access_logs + server_online_players 行数之和）
        async {
            sqlx::query_as::<_, LinkedCountRow>(
                    r#"SELECT steam_id, COUNT(*) AS cnt FROM (
                        SELECT steam_id64 AS steam_id FROM player_access_logs WHERE steam_id64 = ANY($1)
                        UNION ALL
                        SELECT steam_id64 AS steam_id FROM server_online_players WHERE steam_id64 = ANY($1)
                        UNION ALL
                        SELECT steam_id64 AS steam_id FROM player_server_sessions WHERE steam_id64 = ANY($1)
                    ) AS c
                    GROUP BY steam_id"#,
                )
                .bind(linked_ids)
                .fetch_all(&db.pool)
                .await
                .map_err(anyhow::Error::from)
        },
    )?;

    // 合并玩家名（优先级：access_log > online > session > ban > whitelist）
    let mut name_map: HashMap<String, String> = HashMap::new();
    let merge = |map: &mut HashMap<String, String>, rows: Vec<LinkedNameRow>| {
        for row in rows {
            if let Some(name) = row.player_name {
                // 低优先级不覆盖高优先级已写入的名字
                map.entry(row.steam_id64).or_insert(name);
            }
        }
    };
    merge(&mut name_map, name_access);
    merge(&mut name_map, name_online);
    merge(&mut name_map, name_session);
    merge(&mut name_map, name_ban);
    merge(&mut name_map, name_wl);

    let ban_map: HashMap<String, bool> = ban_rows
        .into_iter()
        .map(|r| (r.steam_id, r.cnt > 0))
        .collect();
    let global_ban_map: HashMap<String, bool> = global_ban_rows
        .into_iter()
        .map(|r| (r.steam_id, r.cnt > 0))
        .collect();
    let synced_global_ban_map: HashMap<String, bool> = synced_global_ban_rows
        .into_iter()
        .map(|r| (r.steam_id, r.cnt > 0))
        .collect();

    let whitelist_map: HashMap<String, LinkedWhitelistRow> = whitelist_rows
        .into_iter()
        .map(|r| (r.steam_id.clone(), r))
        .collect();

    // 关联服务器按账号分组
    let mut servers_by_steam: HashMap<String, Vec<String>> = HashMap::new();
    for row in server_rows {
        if let Some(name) = row.server_name {
            servers_by_steam.entry(row.steam_id).or_default().push(name);
        }
    }

    let time_map: HashMap<String, Option<DateTime<Utc>>> = time_rows
        .into_iter()
        .map(|r| (r.steam_id, r.last_seen))
        .collect();

    let count_map: HashMap<String, i64> = count_rows
        .into_iter()
        .map(|r| (r.steam_id, r.cnt))
        .collect();

    // 组装 LinkedAccount 映射（覆盖所有 linked_ids，缺失字段使用默认值）
    let mut account_map = HashMap::with_capacity(linked_ids.len());
    for id in linked_ids {
        account_map.insert(id.clone(), {
            let whitelist = whitelist_map.get(id);
            let (
                whitelist_status,
                whitelist_count,
                whitelist_applied_at,
                whitelist_reviewed_at,
                whitelist_reviewer,
            ) = whitelist
                .map(|row| {
                    (
                        Some(row.status.clone()),
                        row.whitelist_count,
                        Some(row.applied_at),
                        row.reviewed_at,
                        row.reviewer.clone(),
                    )
                })
                .unwrap_or((None, 0, None, None, None));

            LinkedAccount {
                steam_id64: id.clone(),
                player_name: name_map.get(id).cloned(),
                last_seen: time_map.get(id).copied().flatten(),
                has_local_ban: ban_map.get(id).copied().unwrap_or(false),
                has_global_ban: global_ban_map.get(id).copied().unwrap_or(false)
                    || synced_global_ban_map.get(id).copied().unwrap_or(false),
                whitelist_status,
                whitelist_count,
                whitelist_applied_at,
                whitelist_reviewed_at,
                whitelist_reviewer,
                servers: servers_by_steam.remove(id).unwrap_or_default(),
                access_count: count_map.get(id).copied().unwrap_or(0),
            }
        });
    }
    Ok(account_map)
}

async fn fetch_evidence_files(
    db: &Database,
    r2: Option<&R2Storage>,
    bans: &[PlayerBanRecord],
    appeals: &[PlayerAppealRecord],
    reports: &[PlayerReportRecord],
) -> anyhow::Result<Vec<EvidenceFile>> {
    let mut files = Vec::new();

    let ban_ids: Vec<Uuid> = bans.iter().map(|item| item.id).collect();
    for row in fetch_file_rows(db, "ban_files", "ban_id", &ban_ids).await? {
        let related_title = bans
            .iter()
            .find(|ban| ban.id == row.source_id)
            .map(|ban| format!("封禁: {}", ban.reason))
            .unwrap_or_else(|| "封禁证据".to_string());
        files.push(map_evidence_file(row, "ban", "封禁证据", related_title, r2));
    }

    let appeal_ids: Vec<Uuid> = appeals.iter().map(|item| item.id).collect();
    for row in fetch_file_rows(db, "appeal_files", "appeal_id", &appeal_ids).await? {
        let related_title = appeals
            .iter()
            .find(|appeal| appeal.id == row.source_id)
            .map(|appeal| format!("申诉: {}", appeal.appeal_reason))
            .unwrap_or_else(|| "申诉证据".to_string());
        files.push(map_evidence_file(
            row,
            "appeal",
            "申诉证据",
            related_title,
            r2,
        ));
    }

    let report_ids: Vec<Uuid> = reports.iter().map(|item| item.id).collect();
    for row in fetch_file_rows(db, "player_report_files", "report_id", &report_ids).await? {
        let related_title = reports
            .iter()
            .find(|report| report.id == row.source_id)
            .map(|report| format!("举报: {}", report.report_reason))
            .unwrap_or_else(|| "举报证据".to_string());
        files.push(map_evidence_file(
            row,
            "report",
            "举报证据",
            related_title,
            r2,
        ));
    }

    files.sort_by_key(|b| std::cmp::Reverse(b.uploaded_at));
    Ok(files)
}

async fn fetch_file_rows(
    db: &Database,
    table_name: &str,
    foreign_key: &str,
    ids: &[Uuid],
) -> anyhow::Result<Vec<EvidenceFileRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let uploaded_by_select = if table_name == "ban_files" {
        "uploaded_by"
    } else {
        "NULL::TEXT AS uploaded_by"
    };
    let sql = format!(
        r#"SELECT {foreign_key} AS source_id, id, file_name, file_size, content_type,
                  storage_key, category, tags, note, {uploaded_by_select}, uploaded_at
           FROM {table_name}
           WHERE {foreign_key} = ANY($1)
           ORDER BY uploaded_at DESC"#
    );

    let rows = sqlx::query_as::<_, EvidenceFileRow>(&sql)
        .bind(ids)
        .fetch_all(&db.pool)
        .await?;
    Ok(rows)
}

fn map_evidence_file(
    row: EvidenceFileRow,
    source_type: &str,
    source_label: &str,
    related_title: String,
    r2: Option<&R2Storage>,
) -> EvidenceFile {
    EvidenceFile {
        source_type: source_type.to_string(),
        source_id: row.source_id,
        source_label: source_label.to_string(),
        related_title,
        id: row.id,
        file_name: row.file_name,
        file_size: row.file_size,
        content_type: row.content_type,
        category: row.category,
        tags: row.tags,
        note: row.note,
        uploaded_by: row.uploaded_by,
        uploaded_at: row.uploaded_at,
        url: r2.map(|storage| storage.presigned_url(&row.storage_key, 3600)),
    }
}

fn evidence_table(source_type: &str) -> anyhow::Result<(&'static str, &'static str, &'static str)> {
    match source_type {
        "ban" => Ok(("ban_files", "ban_id", "封禁证据")),
        "appeal" => Ok(("appeal_files", "appeal_id", "申诉证据")),
        "report" => Ok(("player_report_files", "report_id", "举报证据")),
        _ => anyhow::bail!("证据来源无效"),
    }
}

fn normalize_note(note: Option<String>, max_len: usize) -> anyhow::Result<String> {
    let note = note.unwrap_or_default().trim().to_string();
    anyhow::ensure!(note.chars().count() <= max_len, "备注过长");
    Ok(note)
}

fn normalize_tags(tags: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim().trim_start_matches('#').to_string();
        if tag.is_empty() {
            continue;
        }
        anyhow::ensure!(tag.chars().count() <= 24, "标签不能超过 24 个字符");
        let key = tag.to_lowercase();
        if seen.insert(key) {
            normalized.push(tag);
        }
        anyhow::ensure!(normalized.len() <= 20, "最多设置 20 个标签");
    }
    Ok(normalized)
}

async fn fetch_admin_actions(
    db: &Database,
    search_terms: &[String],
) -> anyhow::Result<Vec<AdminAction>> {
    if search_terms.is_empty() {
        return Ok(Vec::new());
    }

    let conditions = search_terms
        .iter()
        .enumerate()
        .map(|(idx, _)| format!("l.target_detail ILIKE ${}", idx + 1))
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!(
        r#"SELECT COALESCE(operator_user.display_name, l.operator_name) AS operator_name,
                  l.module, l.action, l.target_detail, l.ip_address, l.created_at
           FROM admin_logs l
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = l.operator_name
                OR u.display_name = l.operator_name
                OR NULLIF(u.remark, '') = l.operator_name
             ORDER BY CASE WHEN u.username = l.operator_name THEN 0 WHEN u.display_name = l.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) operator_user ON true
           WHERE {conditions}
           ORDER BY l.created_at DESC
           LIMIT 100"#
    );

    let mut query = sqlx::query_as::<_, AdminAction>(&sql);
    for term in search_terms {
        query = query.bind(format!("%{term}%"));
    }
    Ok(query.fetch_all(&db.pool).await?)
}

async fn fetch_audit_logs(
    db: &Database,
    search_terms: &[String],
) -> anyhow::Result<Vec<PlayerAuditLog>> {
    if search_terms.is_empty() {
        return Ok(Vec::new());
    }

    let conditions = search_terms
        .iter()
        .enumerate()
        .map(|(idx, _)| {
            let param = idx + 1;
            format!(
                "(al.target ILIKE ${param} OR al.player_name ILIKE ${param} OR al.operator_steamid ILIKE ${param})"
            )
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!(
        r#"SELECT al.id, al.operation, al.target, al.target_type, al.player_name,
                  al.reason, al.duration_minutes,
                  COALESCE(operator_user.display_name, al.operator_name) AS operator_name,
                  al.operator_steamid, al.source, al.server_id, al.server_name, al.server_port,
                  al.success, al.message, al.idempotency_key, al.created_at
           FROM audit_logs al
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = al.operator_name
                OR u.display_name = al.operator_name
                OR NULLIF(u.remark, '') = al.operator_name
             ORDER BY CASE WHEN u.username = al.operator_name THEN 0 WHEN u.display_name = al.operator_name THEN 1 ELSE 2 END
             LIMIT 1
           ) operator_user ON true
           WHERE {conditions}
           ORDER BY al.created_at DESC
           LIMIT 100"#
    );

    let mut query = sqlx::query_as::<_, PlayerAuditLog>(&sql);
    for term in search_terms {
        query = query.bind(format!("%{term}%"));
    }
    Ok(query.fetch_all(&db.pool).await?)
}

struct SearchTermSources<'a> {
    identity: &'a ParsedSteamIdentity,
    whitelists: &'a [WhitelistRecord],
    bans: &'a [PlayerBanRecord],
    appeals: &'a [PlayerAppealRecord],
    reports: &'a [PlayerReportRecord],
    online_records: &'a [OnlineRecord],
    player_sessions: &'a [PlayerServerSession],
    access_logs: &'a [PlayerAccessRecord],
}

fn build_search_terms(sources: SearchTermSources<'_>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    add_search_term(
        &mut terms,
        &mut seen,
        Some(sources.identity.steamid64.as_str()),
    );
    add_search_term(&mut terms, &mut seen, sources.identity.steamid.as_deref());
    add_search_term(&mut terms, &mut seen, sources.identity.steamid3.as_deref());

    for item in sources.whitelists {
        add_search_term(&mut terms, &mut seen, Some(&item.nickname));
        add_search_term(&mut terms, &mut seen, item.steam_persona_name.as_deref());
    }
    for item in sources.bans {
        add_search_term(&mut terms, &mut seen, item.player.as_deref());
    }
    for item in sources.appeals {
        add_search_term(&mut terms, &mut seen, Some(&item.player_name));
    }
    for item in sources.reports {
        add_search_term(&mut terms, &mut seen, item.target_player_name.as_deref());
    }
    for item in sources.online_records {
        add_search_term(&mut terms, &mut seen, Some(&item.name));
    }
    for item in sources.player_sessions {
        add_search_term(&mut terms, &mut seen, item.player_name.as_deref());
    }
    for item in sources.access_logs {
        add_search_term(&mut terms, &mut seen, item.player_name.as_deref());
    }

    terms.truncate(12);
    terms
}

fn add_search_term(terms: &mut Vec<String>, seen: &mut HashSet<String>, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| value.len() >= 2) else {
        return;
    };
    let key = value.to_lowercase();
    if seen.insert(key) {
        terms.push(value.to_string());
    }
}

fn display_name(
    whitelists: &[WhitelistRecord],
    bans: &[PlayerBanRecord],
    appeals: &[PlayerAppealRecord],
    reports: &[PlayerReportRecord],
    online_records: &[OnlineRecord],
    player_sessions: &[PlayerServerSession],
) -> Option<String> {
    whitelists
        .iter()
        .find_map(|item| item.steam_persona_name.clone())
        .or_else(|| whitelists.iter().map(|item| item.nickname.clone()).next())
        .or_else(|| online_records.iter().map(|item| item.name.clone()).next())
        .or_else(|| {
            player_sessions
                .iter()
                .find_map(|item| item.player_name.clone())
        })
        .or_else(|| bans.iter().find_map(|item| item.player.clone()))
        .or_else(|| {
            reports
                .iter()
                .find_map(|item| item.target_player_name.clone())
        })
        .or_else(|| appeals.iter().map(|item| item.player_name.clone()).next())
}

fn is_active_ban(ban: &PlayerBanRecord) -> bool {
    if ban.status != "active" {
        return false;
    }
    match ban.expires_at {
        Some(expires_at) => expires_at > Utc::now(),
        None => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_timeline(
    whitelists: &[WhitelistRecord],
    bans: &[PlayerBanRecord],
    appeals: &[PlayerAppealRecord],
    reports: &[PlayerReportRecord],
    online_records: &[OnlineRecord],
    player_sessions: &[PlayerServerSession],
    access_logs: &[PlayerAccessRecord],
    admin_actions: &[AdminAction],
    audit_logs: &[PlayerAuditLog],
    evidence_files: &[EvidenceFile],
    map_feedback: &[MapFeedbackItem],
) -> Vec<TimelineEvent> {
    let mut events = Vec::new();

    for item in whitelists {
        events.push(TimelineEvent {
            event_type: "whitelist_applied".to_string(),
            category: "whitelist".to_string(),
            title: "提交白名单申请".to_string(),
            description: Some(format!("昵称: {}", item.nickname)),
            occurred_at: item.applied_at,
            actor: None,
            status: Some(item.status.clone()),
            source: item.source.clone(),
            related_id: Some(item.id),
        });
        if let Some(approved_at) = item.approved_at {
            events.push(TimelineEvent {
                event_type: "whitelist_approved".to_string(),
                category: "whitelist".to_string(),
                title: "白名单通过".to_string(),
                description: item.approval_reason.clone(),
                occurred_at: approved_at,
                actor: item.approved_by.clone(),
                status: Some("approved".to_string()),
                source: item.source.clone(),
                related_id: Some(item.id),
            });
        }
        if let Some(rejected_at) = item.rejected_at {
            events.push(TimelineEvent {
                event_type: "whitelist_rejected".to_string(),
                category: "whitelist".to_string(),
                title: "白名单拒绝".to_string(),
                description: item.rejection_reason.clone(),
                occurred_at: rejected_at,
                actor: item.rejected_by.clone(),
                status: Some("rejected".to_string()),
                source: item.source.clone(),
                related_id: Some(item.id),
            });
        }
        if let Some(revoked_at) = item.revoked_at {
            events.push(TimelineEvent {
                event_type: "whitelist_revoked".to_string(),
                category: "whitelist".to_string(),
                title: "白名单撤销".to_string(),
                description: None,
                occurred_at: revoked_at,
                actor: item.revoked_by.clone(),
                status: Some("revoked".to_string()),
                source: item.source.clone(),
                related_id: Some(item.id),
            });
        }
    }

    for item in bans {
        events.push(TimelineEvent {
            event_type: "ban_created".to_string(),
            category: "ban".to_string(),
            title: "创建封禁".to_string(),
            description: Some(item.reason.clone()),
            occurred_at: item.created_at,
            actor: Some(item.operator_name.clone()),
            status: Some(item.status.clone()),
            source: Some(item.source.clone()),
            related_id: Some(item.id),
        });
        if let Some(removed_at) = item.removed_at {
            events.push(TimelineEvent {
                event_type: "ban_removed".to_string(),
                category: "ban".to_string(),
                title: "解除封禁".to_string(),
                description: item.removed_reason.clone(),
                occurred_at: removed_at,
                actor: item.removed_by.clone(),
                status: Some("inactive".to_string()),
                source: Some(item.source.clone()),
                related_id: Some(item.id),
            });
        }
    }

    for item in appeals {
        events.push(TimelineEvent {
            event_type: "appeal_created".to_string(),
            category: "appeal".to_string(),
            title: "提交封禁申诉".to_string(),
            description: Some(item.appeal_reason.clone()),
            occurred_at: item.created_at,
            actor: None,
            status: Some(item.status.clone()),
            source: None,
            related_id: Some(item.id),
        });
        if let Some(reviewed_at) = item.reviewed_at {
            events.push(TimelineEvent {
                event_type: "appeal_reviewed".to_string(),
                category: "appeal".to_string(),
                title: "处理封禁申诉".to_string(),
                description: item.review_note.clone(),
                occurred_at: reviewed_at,
                actor: item.reviewed_by.clone(),
                status: Some(item.status.clone()),
                source: None,
                related_id: Some(item.id),
            });
        }
    }

    for item in reports {
        events.push(TimelineEvent {
            event_type: "report_created".to_string(),
            category: "report".to_string(),
            title: "收到玩家举报".to_string(),
            description: Some(item.report_reason.clone()),
            occurred_at: item.created_at,
            actor: None,
            status: Some(item.status.clone()),
            source: None,
            related_id: Some(item.id),
        });
        if let Some(reviewed_at) = item.reviewed_at {
            events.push(TimelineEvent {
                event_type: "report_reviewed".to_string(),
                category: "report".to_string(),
                title: "处理玩家举报".to_string(),
                description: item.review_note.clone(),
                occurred_at: reviewed_at,
                actor: item.reviewed_by.clone(),
                status: Some(item.status.clone()),
                source: None,
                related_id: Some(item.id),
            });
        }
    }

    for item in online_records {
        events.push(TimelineEvent {
            event_type: "online_reported".to_string(),
            category: "online".to_string(),
            title: "在线上报".to_string(),
            description: Some(format!("{} / {}", item.server_name, item.current_map)),
            occurred_at: item.reported_at,
            actor: None,
            status: Some("online".to_string()),
            source: Some("server_online_players".to_string()),
            related_id: Some(item.server_id),
        });
    }

    for item in player_sessions {
        events.push(TimelineEvent {
            event_type: "server_session_started".to_string(),
            category: "session".to_string(),
            title: "进入服务器".to_string(),
            description: Some(format!(
                "{}:{} / {}",
                item.server_name,
                item.server_port,
                if item.last_map.is_empty() {
                    "-"
                } else {
                    item.last_map.as_str()
                }
            )),
            occurred_at: item.first_seen_at,
            actor: None,
            status: Some(if item.left_at.is_some() {
                "inactive".to_string()
            } else {
                "online".to_string()
            }),
            source: Some("player_server_sessions".to_string()),
            related_id: Some(item.id),
        });
        if let Some(left_at) = item.left_at {
            events.push(TimelineEvent {
                event_type: "server_session_left".to_string(),
                category: "session".to_string(),
                title: "退出服务器".to_string(),
                description: Some(format!("{}:{}", item.server_name, item.server_port)),
                occurred_at: left_at,
                actor: None,
                status: Some("inactive".to_string()),
                source: item.end_reason.clone(),
                related_id: Some(item.id),
            });
        }
    }

    for item in access_logs {
        events.push(TimelineEvent {
            event_type: if item.allowed {
                "access_allowed".to_string()
            } else {
                "access_denied".to_string()
            },
            category: "access".to_string(),
            title: if item.allowed {
                "进服成功".to_string()
            } else {
                "进服失败".to_string()
            },
            description: Some(if item.allowed {
                format!(
                    "{}:{} / {}",
                    item.server_name, item.server_port, item.access_method
                )
            } else {
                item.reject_reason.clone().unwrap_or_else(|| {
                    item.failure_code
                        .clone()
                        .unwrap_or_else(|| "未知失败原因".to_string())
                })
            }),
            occurred_at: item.created_at,
            actor: item.player_name.clone(),
            status: Some(if item.allowed { "success" } else { "failed" }.to_string()),
            source: Some(item.access_method.clone()),
            related_id: Some(item.id),
        });
    }

    for item in map_feedback {
        if let Some(created_at) = parse_rfc3339_utc(&item.created_at) {
            events.push(TimelineEvent {
                event_type: "map_feedback_created".to_string(),
                category: "map_feedback".to_string(),
                title: "提交地图反馈".to_string(),
                description: Some(item.detail.clone()),
                occurred_at: created_at,
                actor: item.steam_persona_name.clone(),
                status: Some(item.status.clone()),
                source: Some(item.feedback_type.clone()),
                related_id: Some(item.id),
            });
        }
        if let Some(reviewed_at) = item.reviewed_at.as_deref().and_then(parse_rfc3339_utc) {
            events.push(TimelineEvent {
                event_type: "map_feedback_reviewed".to_string(),
                category: "map_feedback".to_string(),
                title: "处理地图反馈".to_string(),
                description: item.review_note.clone(),
                occurred_at: reviewed_at,
                actor: item.reviewed_by.clone(),
                status: Some(item.status.clone()),
                source: Some(item.feedback_type.clone()),
                related_id: Some(item.id),
            });
        }
    }

    for item in admin_actions {
        events.push(TimelineEvent {
            event_type: "admin_action".to_string(),
            category: "admin".to_string(),
            title: format!("{} / {}", item.module, item.action),
            description: Some(item.target_detail.clone()),
            occurred_at: item.created_at,
            actor: Some(item.operator_name.clone()),
            status: None,
            source: Some("admin_logs".to_string()),
            related_id: None,
        });
    }

    for item in audit_logs {
        events.push(TimelineEvent {
            event_type: "audit_log".to_string(),
            category: "audit".to_string(),
            title: format!("审计: {}", item.operation),
            description: item
                .message
                .clone()
                .or_else(|| item.reason.clone())
                .or_else(|| item.player_name.clone()),
            occurred_at: item.created_at,
            actor: Some(item.operator_name.clone()),
            status: Some(if item.success { "success" } else { "failed" }.to_string()),
            source: Some(item.source.clone()),
            related_id: Some(item.id),
        });
    }

    for item in evidence_files {
        events.push(TimelineEvent {
            event_type: "evidence_uploaded".to_string(),
            category: "evidence".to_string(),
            title: format!("上传{}", item.source_label),
            description: Some(item.file_name.clone()),
            occurred_at: item.uploaded_at,
            actor: item.uploaded_by.clone(),
            status: Some(item.category.clone()),
            source: Some(item.source_type.clone()),
            related_id: Some(item.id),
        });
    }

    events.sort_by_key(|b| std::cmp::Reverse(b.occurred_at));
    events.truncate(250);
    events
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}
