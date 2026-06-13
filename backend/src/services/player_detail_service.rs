use crate::{
    db::Database,
    services::{
        r2_storage::R2Storage,
        steam_service::{ParsedSteamIdentity, SteamResolver},
    },
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct PlayerDetail {
    pub profile: PlayerProfile,
    pub summary: PlayerSummary,
    pub whitelist: Vec<WhitelistRecord>,
    pub bans: Vec<PlayerBanRecord>,
    pub appeals: Vec<PlayerAppealRecord>,
    pub reports: Vec<PlayerReportRecord>,
    pub online_records: Vec<OnlineRecord>,
    pub ip_history: Vec<IpHistoryEntry>,
    pub admin_actions: Vec<AdminAction>,
    pub audit_logs: Vec<PlayerAuditLog>,
    pub evidence_files: Vec<EvidenceFile>,
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

#[derive(Debug, Serialize)]
pub struct PlayerSummary {
    pub whitelist_status: Option<String>,
    pub active_ban_count: usize,
    pub ban_count: usize,
    pub appeal_count: usize,
    pub report_count: usize,
    pub online_server_count: usize,
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

/// 使用同一 IP 的关联账号
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LinkedAccount {
    pub steam_id64: String,
    pub player_name: Option<String>,
    pub server_name: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
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
    let identity = resolver.resolve(steam_input).await?;
    let steamid64 = identity.steamid64.clone();

    let whitelists = fetch_whitelist_records(db, &steamid64).await?;
    let bans = fetch_ban_records(db, &steamid64).await?;
    let appeals = fetch_appeal_records(db, &steamid64).await?;
    let reports = fetch_report_records(db, &steamid64).await?;
    let online_records = fetch_online_records(db, &steamid64).await?;
    let ip_history = fetch_ip_history(db, &steamid64).await?;
    let evidence_files = fetch_evidence_files(db, r2, &bans, &appeals, &reports).await?;
    let internal_profile = fetch_internal_profile(db, &steamid64).await?;

    let search_terms = build_search_terms(
        &identity,
        &whitelists,
        &bans,
        &appeals,
        &reports,
        &online_records,
    );
    let admin_actions = fetch_admin_actions(db, &search_terms).await?;
    let audit_logs = fetch_audit_logs(db, &search_terms).await?;

    let display_name = display_name(&whitelists, &bans, &appeals, &reports, &online_records);
    let profile = PlayerProfile {
        steamid64: steamid64.clone(),
        steamid: identity.steamid,
        steamid3: identity.steamid3,
        profile_url: identity.profile_url,
        display_name,
    };
    let summary = PlayerSummary {
        whitelist_status: whitelists.first().map(|item| item.status.clone()),
        active_ban_count: bans.iter().filter(|ban| is_active_ban(ban)).count(),
        ban_count: bans.len(),
        appeal_count: appeals.len(),
        report_count: reports.len(),
        online_server_count: online_records.len(),
        evidence_file_count: evidence_files.len(),
        admin_action_count: admin_actions.len() + audit_logs.len(),
        last_seen_at: online_records.first().map(|item| item.reported_at),
    };

    let timeline = build_timeline(
        &whitelists,
        &bans,
        &appeals,
        &reports,
        &online_records,
        &admin_actions,
        &audit_logs,
        &evidence_files,
    );

    Ok(PlayerDetail {
        profile,
        summary,
        whitelist: whitelists,
        bans,
        appeals,
        reports,
        online_records,
        ip_history,
        admin_actions,
        audit_logs,
        evidence_files,
        internal_profile,
        timeline,
    })
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
        r#"SELECT wr.id, wr.steamid64, wr.steamid, wr.steamid3, wr.profile_url,
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

/// 查询玩家使用过的 IP 地址历史，包括：
/// - 每个 IP 登录过哪些服务器
/// - 每个 IP 关联的其他账号（同 IP 不同 SteamID64）
///
/// 数据来源：player_access_logs + server_online_players + ban_records
async fn fetch_ip_history(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Vec<IpHistoryEntry>> {
    // 1) 收集该玩家所有使用过的 IP（来源：player_access_logs + server_online_players + ban_records）
    //    使用 UNION 去重后获取唯一的 IP 列表
    let ip_rows: Vec<(String,)> = sqlx::query_as(
        r#"SELECT DISTINCT ip FROM (
            SELECT ip_address AS ip FROM player_access_logs
            WHERE steam_id64 = $1 AND ip_address IS NOT NULL AND btrim(ip_address) <> ''
            UNION
            SELECT ip FROM server_online_players
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

    let mut entries = Vec::with_capacity(player_ips.len());
    for ip in &player_ips {
        // 2) 查询该 IP 在哪些服务器上登录过（该玩家自己的记录）
        let server_rows: Vec<IpServerRecord> = sqlx::query_as::<_, IpServerRecord>(
            r#"SELECT * FROM (
                SELECT server_name, server_port::INTEGER AS server_port, player_name,
                       max(created_at) AS last_seen, 'access_log' AS source
                FROM player_access_logs
                WHERE ip_address = $1 AND steam_id64 = $2
                GROUP BY server_name, server_port, player_name
                UNION ALL
                SELECT s.name AS server_name, sop.server_port::INTEGER AS server_port,
                       sop.name AS player_name, max(sop.reported_at) AS last_seen,
                       'online_report' AS source
                FROM server_online_players sop
                JOIN servers s ON s.id = sop.server_id
                WHERE sop.ip = $1 AND sop.steam_id64 = $2
                GROUP BY s.name, sop.server_port, sop.name
            ) AS combined
            ORDER BY last_seen DESC
            LIMIT 50"#,
        )
        .bind(ip)
        .bind(steamid64)
        .fetch_all(&db.pool)
        .await?;

        // 3) 计算该玩家使用此 IP 的最早/最晚时间
        let times: Option<(Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = sqlx::query_as(
            r#"SELECT min(ts) AS first_seen, max(ts) AS last_seen FROM (
                SELECT created_at AS ts FROM player_access_logs WHERE ip_address = $1 AND steam_id64 = $2
                UNION ALL
                SELECT reported_at AS ts FROM server_online_players WHERE ip = $1 AND steam_id64 = $2
                UNION ALL
                SELECT created_at AS ts FROM ban_records WHERE ip_address = $1 AND steam_id = $2
            ) AS times"#,
        )
        .bind(ip)
        .bind(steamid64)
        .fetch_optional(&db.pool)
        .await?;
        let (first_seen, last_seen) = times.unwrap_or((None, None));

        // 4) 查询使用过同一 IP 的其他账号（不同 SteamID64）
        let linked_rows: Vec<LinkedAccount> = sqlx::query_as::<_, LinkedAccount>(
            r#"SELECT * FROM (
                SELECT DISTINCT ON (steam_id64) steam_id64, player_name, server_name, last_seen FROM (
                    SELECT steam_id64, player_name, server_name, created_at AS last_seen
                    FROM player_access_logs
                    WHERE ip_address = $1 AND steam_id64 <> $2
                    UNION ALL
                    SELECT sop.steam_id64, sop.name AS player_name, s.name AS server_name, sop.reported_at AS last_seen
                    FROM server_online_players sop
                    JOIN servers s ON s.id = sop.server_id
                    WHERE sop.ip = $1 AND sop.steam_id64 <> $2
                    UNION ALL
                    SELECT steam_id AS steam_id64, player AS player_name, NULL::TEXT AS server_name, created_at AS last_seen
                    FROM ban_records
                    WHERE ip_address = $1 AND steam_id <> $2
                ) AS combined
                ORDER BY steam_id64, last_seen DESC
            ) AS unique_accounts
            ORDER BY last_seen DESC
            LIMIT 30"#,
        )
        .bind(ip)
        .bind(steamid64)
        .fetch_all(&db.pool)
        .await?;

        entries.push(IpHistoryEntry {
            ip: ip.clone(),
            servers: server_rows,
            first_seen,
            last_seen,
            linked_accounts: linked_rows,
        });
    }

    // 按 last_seen 降序排列
    #[allow(clippy::unnecessary_sort_by)]
    entries.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    Ok(entries)
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

    files.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
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

fn build_search_terms(
    identity: &ParsedSteamIdentity,
    whitelists: &[WhitelistRecord],
    bans: &[PlayerBanRecord],
    appeals: &[PlayerAppealRecord],
    reports: &[PlayerReportRecord],
    online_records: &[OnlineRecord],
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    add_search_term(&mut terms, &mut seen, Some(identity.steamid64.as_str()));
    add_search_term(&mut terms, &mut seen, identity.steamid.as_deref());
    add_search_term(&mut terms, &mut seen, identity.steamid3.as_deref());

    for item in whitelists {
        add_search_term(&mut terms, &mut seen, Some(&item.nickname));
        add_search_term(&mut terms, &mut seen, item.steam_persona_name.as_deref());
    }
    for item in bans {
        add_search_term(&mut terms, &mut seen, item.player.as_deref());
    }
    for item in appeals {
        add_search_term(&mut terms, &mut seen, Some(&item.player_name));
    }
    for item in reports {
        add_search_term(&mut terms, &mut seen, item.target_player_name.as_deref());
    }
    for item in online_records {
        add_search_term(&mut terms, &mut seen, Some(&item.name));
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
) -> Option<String> {
    whitelists
        .iter()
        .find_map(|item| item.steam_persona_name.clone())
        .or_else(|| {
            whitelists
                .iter()
                .find_map(|item| Some(item.nickname.clone()))
        })
        .or_else(|| {
            online_records
                .iter()
                .find_map(|item| Some(item.name.clone()))
        })
        .or_else(|| bans.iter().find_map(|item| item.player.clone()))
        .or_else(|| {
            reports
                .iter()
                .find_map(|item| item.target_player_name.clone())
        })
        .or_else(|| {
            appeals
                .iter()
                .find_map(|item| Some(item.player_name.clone()))
        })
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

fn build_timeline(
    whitelists: &[WhitelistRecord],
    bans: &[PlayerBanRecord],
    appeals: &[PlayerAppealRecord],
    reports: &[PlayerReportRecord],
    online_records: &[OnlineRecord],
    admin_actions: &[AdminAction],
    audit_logs: &[PlayerAuditLog],
    evidence_files: &[EvidenceFile],
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

    events.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
    events.truncate(250);
    events
}
