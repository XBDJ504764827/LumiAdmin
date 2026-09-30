use crate::db::Database;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const RECENT_IP_LINK_DAYS: i64 = 90;
const OLD_IP_LINK_DAYS: i64 = 365;
const MAX_LINKED_ACCOUNT_ITEMS: usize = 20;

/// 封禁类风险信号：进服准入只认这些信号（自身有效封禁 / 同 IP 关联账号有效封禁
/// / 同 QQ 关联账号有效封禁）。
pub const BAN_RISK_REASON_CODES: [&str; 7] = [
    "self_active_local_ban",
    "self_synced_global_local_ban",
    "self_active_global_ban",
    "linked_ip_local_ban",
    "linked_ip_global_ban",
    "linked_qq_local_ban",
    "linked_qq_global_ban",
];

pub fn is_ban_risk_reason_code(code: &str) -> bool {
    BAN_RISK_REASON_CODES.contains(&code)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    Info,
    Warning,
    Block,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskAction {
    Allow,
    Warn,
    RequireForce,
    Deny,
}

impl RiskAction {
    /// 风险等级中文标签：allow=低风险、warn=中风险、require_force/deny=高风险。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "低风险",
            Self::Warn => "中风险",
            Self::RequireForce | Self::Deny => "高风险",
        }
    }
}

/// 进服准入使用的封禁类风险评估结果。
///
/// 与后台「玩家风险档案」使用同一套信号与分级规则，但只统计封禁类信号，
/// 不计算白名单拒绝次数、共享 IP 账号数等次要信号，避免在进服热点路径上
/// 产生额外查询。
#[derive(Debug, Clone)]
pub struct AccessBanRisk {
    /// 命中的封禁类信号代码（与风险档案的 code 一致）
    pub codes: Vec<String>,
    /// 综合风险动作（分级规则与风险档案一致）
    pub action: RiskAction,
    /// 命中信号的中文明细，用于后台审计/进服日志
    pub detail: String,
}

impl AccessBanRisk {
    /// 中高风险（warn / require_force / deny）账号需要白名单才可进入。
    pub fn is_medium_or_high(&self) -> bool {
        self.action != RiskAction::Allow
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerRiskProfile {
    pub steamid64: String,
    pub action: RiskAction,
    pub severity: RiskSeverity,
    pub summary: String,
    pub recommendation: String,
    pub reasons: Vec<RiskReason>,
    pub linked_accounts: Vec<RiskLinkedAccount>,
    pub shared_ip_count: usize,
    pub linked_account_count: usize,
    pub linked_banned_account_count: usize,
    pub linked_global_banned_account_count: usize,
    /// 当前账号绑定的 QQ openid（未绑定时为 None）
    #[serde(default)]
    pub qq_openid: Option<String>,
    /// 同 QQ openid 下绑定的其他 Steam 账号（不含自己）
    #[serde(default)]
    pub qq_linked_accounts: Vec<RiskLinkedAccount>,
    #[serde(default)]
    pub qq_linked_account_count: usize,
    #[serde(default)]
    pub qq_linked_banned_account_count: usize,
    #[serde(default)]
    pub qq_linked_global_banned_account_count: usize,
}

impl PlayerRiskProfile {
    pub fn requires_force(&self) -> bool {
        self.action == RiskAction::RequireForce
    }

    pub fn denies(&self) -> bool {
        self.action == RiskAction::Deny
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskReason {
    pub code: String,
    pub severity: RiskSeverity,
    pub message: String,
    pub steamid64: Option<String>,
    pub ip: Option<String>,
    pub count: i64,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskLinkedAccount {
    pub steamid64: String,
    pub player_name: Option<String>,
    pub shared_ips: Vec<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub has_active_local_ban: bool,
    pub has_active_global_ban: bool,
    pub rejected_whitelist_count: i64,
    /// 是否经由 QQ openid 关联（true = 同 QQ 绑定，false = 同 IP 关联）
    #[serde(default)]
    pub via_qq: bool,
    /// 关联所经由的 QQ openid（via_qq 为 true 时有值）
    #[serde(default)]
    pub qq_openid: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActiveBanRow {
    steam_id: String,
    ip_address: Option<String>,
    reason: String,
    source: String,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct GlobalBanRow {
    steam_id64: String,
    player_name: Option<String>,
    ban_type: String,
    notes: Option<String>,
    created_on: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct IpUsageRow {
    ip: String,
    steam_id: String,
    player_name: Option<String>,
    last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LinkedSignalRow {
    steam_id: String,
    rejected_whitelist_count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct QqSiblingRow {
    steam_id: String,
    qq_openid: String,
    qq_username: Option<String>,
    whitelist_name: Option<String>,
    verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct QqActivityRow {
    steam_id: String,
    last_seen: Option<DateTime<Utc>>,
}

pub async fn build_player_risk_profile(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<PlayerRiskProfile> {
    let steamid64 = steamid64.trim();
    anyhow::ensure!(
        steamid64.len() == 17 && steamid64.chars().all(|ch| ch.is_ascii_digit()),
        "SteamID64 格式无效"
    );

    let self_steamids = vec![steamid64.to_string()];
    let (self_bans, self_global_bans, player_ip_rows, qq_openid) = tokio::try_join!(
        load_active_bans_for_steamids(db, &self_steamids),
        load_active_global_bans(db, &self_steamids),
        load_player_ips(db, steamid64),
        load_qq_openid_for_steam(db, steamid64),
    )?;

    let player_ips: Vec<String> = player_ip_rows.into_iter().map(|(ip,)| ip).collect();
    let ip_count = player_ips.len();
    let linked_rows = load_linked_accounts_by_ips(db, steamid64, &player_ips).await?;

    let mut linked_by_steam: HashMap<String, RiskLinkedAccount> = HashMap::new();
    for row in linked_rows {
        let entry = linked_by_steam
            .entry(row.steam_id.clone())
            .or_insert_with(|| RiskLinkedAccount {
                steamid64: row.steam_id.clone(),
                player_name: row.player_name.clone(),
                shared_ips: Vec::new(),
                last_seen_at: row.last_seen_at,
                has_active_local_ban: false,
                has_active_global_ban: false,
                rejected_whitelist_count: 0,
                via_qq: false,
                qq_openid: None,
            });
        if !entry.shared_ips.iter().any(|ip| ip == &row.ip) {
            entry.shared_ips.push(row.ip);
        }
        if entry.player_name.is_none() {
            entry.player_name = row.player_name;
        }
        entry.last_seen_at = [entry.last_seen_at, row.last_seen_at]
            .into_iter()
            .flatten()
            .max();
    }

    let linked_ids: Vec<String> = linked_by_steam.keys().cloned().collect();
    let (linked_bans, linked_global_bans, linked_signals) = tokio::try_join!(
        load_active_bans_for_steamids(db, &linked_ids),
        load_active_global_bans(db, &linked_ids),
        load_linked_signals(db, &linked_ids),
    )?;

    let linked_local_banned: HashSet<String> =
        linked_bans.iter().map(|row| row.steam_id.clone()).collect();
    let linked_global_banned: HashSet<String> = linked_global_bans
        .iter()
        .map(|row| row.steam_id64.clone())
        .collect();
    let linked_signal_map: HashMap<String, LinkedSignalRow> = linked_signals
        .into_iter()
        .map(|row| (row.steam_id.clone(), row))
        .collect();

    for (steam_id, account) in linked_by_steam.iter_mut() {
        account.has_active_local_ban = linked_local_banned.contains(steam_id);
        account.has_active_global_ban = linked_global_banned.contains(steam_id);
        if let Some(signal) = linked_signal_map.get(steam_id) {
            account.rejected_whitelist_count = signal.rejected_whitelist_count;
        }
    }

    let mut linked_accounts: Vec<RiskLinkedAccount> = linked_by_steam.into_values().collect();
    linked_accounts.sort_by(|a, b| {
        let a_score = linked_account_score(a);
        let b_score = linked_account_score(b);
        b_score
            .cmp(&a_score)
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
            .then_with(|| a.steamid64.cmp(&b.steamid64))
    });

    // 同 QQ openid 关联：同一 QQ 下绑定的其他 Steam 账号。
    // QQ 绑定是强身份关联（1 个 QQ 最多绑定 N 个 Steam），任一同 QQ 账号存在
    // 本地/全球封禁或白名单拒绝记录时，当前 QQ 也视为高风险。
    let mut qq_linked_accounts: Vec<RiskLinkedAccount> = Vec::new();
    if let Some(ref openid) = qq_openid {
        qq_linked_accounts = load_qq_linked_accounts(db, openid, steamid64).await?;
    }

    let mut reasons = Vec::new();
    add_self_ban_reasons(&mut reasons, &self_bans);
    add_self_global_ban_reasons(&mut reasons, &self_global_bans);
    add_linked_account_reasons(&mut reasons, &linked_accounts);
    add_qq_linked_account_reasons(&mut reasons, &qq_linked_accounts);

    // 已展示全球封禁原因时，去掉重复的“由全球封禁同步生成的本地封禁”提示
    if reasons
        .iter()
        .any(|reason| reason.code == "self_active_global_ban")
    {
        reasons.retain(|reason| reason.code != "self_synced_global_local_ban");
    }

    let action = classify_action(&reasons);
    let severity = classify_severity(&reasons);
    let linked_banned_account_count = linked_accounts
        .iter()
        .filter(|account| account.has_active_local_ban)
        .count();
    let linked_global_banned_account_count = linked_accounts
        .iter()
        .filter(|account| account.has_active_global_ban)
        .count();
    let linked_account_count = linked_accounts.len();
    let qq_linked_account_count = qq_linked_accounts.len();
    let qq_linked_banned_account_count = qq_linked_accounts
        .iter()
        .filter(|account| account.has_active_local_ban)
        .count();
    let qq_linked_global_banned_account_count = qq_linked_accounts
        .iter()
        .filter(|account| account.has_active_global_ban)
        .count();
    let summary = risk_summary(
        &action,
        &reasons,
        linked_account_count,
        linked_banned_account_count,
        linked_global_banned_account_count,
        qq_linked_account_count,
        qq_linked_banned_account_count,
        qq_linked_global_banned_account_count,
    );
    let recommendation = risk_recommendation(&action).to_string();

    linked_accounts.truncate(MAX_LINKED_ACCOUNT_ITEMS);
    qq_linked_accounts.truncate(MAX_LINKED_ACCOUNT_ITEMS);

    Ok(PlayerRiskProfile {
        steamid64: steamid64.to_string(),
        action,
        severity,
        summary,
        recommendation,
        reasons,
        linked_accounts,
        shared_ip_count: ip_count,
        linked_account_count,
        linked_banned_account_count,
        linked_global_banned_account_count,
        qq_openid,
        qq_linked_account_count,
        qq_linked_banned_account_count,
        qq_linked_global_banned_account_count,
        qq_linked_accounts,
    })
}

/// 进服准入：评估账号的封禁类风险（中高风险需要白名单才可进入）。
///
/// 与后台风险档案保持同一套信号与分级规则：
/// - 自身有效本地封禁 / 由全球封禁同步生成的本地封禁 / 自身有效全球封禁；
/// - 同 IP 关联账号（含本次上报的当前 IP 与历史 IP）存在有效本地/全球封禁；
/// - 同 QQ openid 关联账号（同一 QQ 绑定的其他 Steam）存在有效本地/全球封禁。
///
/// 只统计封禁类信号，自身命中封禁类信号时直接返回（自身封禁必然为高风险，
/// 无需再查询关联账号）；同 QQ 与同 IP 信号会合并后统一判定，保证混合命中时
/// 审计明细与 failure_code 能同时体现两路来源。
pub async fn evaluate_ban_risk_for_access(
    db: &Database,
    steamid64: &str,
    ip_address: Option<&str>,
) -> anyhow::Result<Option<AccessBanRisk>> {
    let steamid64 = steamid64.trim();
    anyhow::ensure!(
        steamid64.len() == 17 && steamid64.chars().all(|ch| ch.is_ascii_digit()),
        "SteamID64 格式无效"
    );
    let current_ip = ip_address.map(str::trim).filter(|ip| !ip.is_empty());

    let self_steamids = vec![steamid64.to_string()];
    let (self_bans, self_global_bans, player_ip_rows, qq_openid) = tokio::try_join!(
        load_active_bans_for_steamids(db, &self_steamids),
        load_active_global_bans(db, &self_steamids),
        load_player_ips(db, steamid64),
        load_qq_openid_for_steam(db, steamid64),
    )?;

    let mut reasons = Vec::new();
    add_self_ban_reasons(&mut reasons, &self_bans);
    add_self_global_ban_reasons(&mut reasons, &self_global_bans);
    if !reasons.is_empty() {
        return Ok(access_ban_risk_from_reasons(reasons));
    }

    // 同 QQ 关联：同一 QQ openid 下绑定的其他 Steam 账号。
    // QQ 绑定是强身份关联，查询成本低（同 QQ 最多数十个账号）。
    // 注意：这里不做“命中即提前返回”，而是把 QQ 信号并入 reasons 后继续查 IP，
    // 保证同 IP + 同 QQ 混合命中时审计明细与 failure_code 能同时体现两路信号。
    if let Some(ref openid) = qq_openid {
        let qq_linked_accounts = load_qq_linked_accounts(db, openid, steamid64).await?;
        add_qq_linked_account_reasons(&mut reasons, &qq_linked_accounts);
    }

    // 同 IP 关联：玩家历史 IP + 本次上报的当前 IP。
    // 首次进服时当前 IP 可能尚未写入历史，必须一并纳入判定，
    // 否则与原先的「同 IP 关联封禁」直接拦截相比会漏判。
    let mut ips: Vec<String> = player_ip_rows.into_iter().map(|(ip,)| ip).collect();
    if let Some(ip) = current_ip {
        if !ips.iter().any(|existing| existing == ip) {
            ips.push(ip.to_string());
        }
    }
    if ips.is_empty() {
        return Ok(access_ban_risk_from_reasons(reasons));
    }

    let linked_rows = load_linked_accounts_by_ips(db, steamid64, &ips).await?;
    if linked_rows.is_empty() {
        return Ok(access_ban_risk_from_reasons(reasons));
    }

    let mut linked_by_steam: HashMap<String, RiskLinkedAccount> = HashMap::new();
    for row in linked_rows {
        let entry = linked_by_steam
            .entry(row.steam_id.clone())
            .or_insert_with(|| RiskLinkedAccount {
                steamid64: row.steam_id.clone(),
                player_name: row.player_name.clone(),
                shared_ips: Vec::new(),
                last_seen_at: row.last_seen_at,
                has_active_local_ban: false,
                has_active_global_ban: false,
                rejected_whitelist_count: 0,
                via_qq: false,
                qq_openid: None,
            });
        if !entry.shared_ips.iter().any(|ip| ip == &row.ip) {
            entry.shared_ips.push(row.ip);
        }
        if entry.player_name.is_none() {
            entry.player_name = row.player_name;
        }
        entry.last_seen_at = [entry.last_seen_at, row.last_seen_at]
            .into_iter()
            .flatten()
            .max();
    }

    let linked_ids: Vec<String> = linked_by_steam.keys().cloned().collect();
    let (linked_bans, linked_global_bans) = tokio::try_join!(
        load_active_bans_for_steamids(db, &linked_ids),
        load_active_global_bans(db, &linked_ids),
    )?;
    let linked_local_banned: HashSet<String> =
        linked_bans.iter().map(|row| row.steam_id.clone()).collect();
    let linked_global_banned: HashSet<String> = linked_global_bans
        .iter()
        .map(|row| row.steam_id64.clone())
        .collect();
    for (steam_id, account) in linked_by_steam.iter_mut() {
        account.has_active_local_ban = linked_local_banned.contains(steam_id);
        account.has_active_global_ban = linked_global_banned.contains(steam_id);
    }

    let mut linked_accounts: Vec<RiskLinkedAccount> = linked_by_steam.into_values().collect();
    linked_accounts.sort_by(|a, b| {
        linked_account_score(b)
            .cmp(&linked_account_score(a))
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
            .then_with(|| a.steamid64.cmp(&b.steamid64))
    });

    add_linked_account_reasons(&mut reasons, &linked_accounts);
    Ok(access_ban_risk_from_reasons(reasons))
}

/// 由风险原因收敛出进服准入结论：只保留封禁类信号，且整体风险需为中/高。
fn access_ban_risk_from_reasons(reasons: Vec<RiskReason>) -> Option<AccessBanRisk> {
    let mut codes = Vec::new();
    let mut details = Vec::new();
    for reason in reasons
        .iter()
        .filter(|reason| is_ban_risk_reason_code(&reason.code))
    {
        if !codes.iter().any(|code| code == &reason.code) {
            codes.push(reason.code.clone());
        }
        details.push(reason.message.clone());
    }
    if codes.is_empty() {
        return None;
    }
    let action = classify_action(&reasons);
    if action == RiskAction::Allow {
        return None;
    }
    Some(AccessBanRisk {
        codes,
        action,
        detail: details.join("；"),
    })
}

async fn load_active_bans_for_steamids(
    db: &Database,
    steamids: &[String],
) -> anyhow::Result<Vec<ActiveBanRow>> {
    if steamids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, ActiveBanRow>(
        r#"SELECT steam_id, ip_address, reason, source, expires_at, created_at
           FROM ban_records
           WHERE steam_id = ANY($1)
             AND status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
           ORDER BY created_at DESC
           LIMIT 500"#,
    )
    .bind(steamids)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn load_active_global_bans(
    db: &Database,
    steamids: &[String],
) -> anyhow::Result<Vec<GlobalBanRow>> {
    if steamids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, GlobalBanRow>(
        r#"SELECT steam_id64, player_name, ban_type, notes, created_on
           FROM global_bans
           WHERE steam_id64 = ANY($1)
             AND is_expired = false
             AND manual_unbanned = false
           ORDER BY created_on DESC NULLS LAST, synced_at DESC
           LIMIT 500"#,
    )
    .bind(steamids)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn load_player_ips(db: &Database, steamid64: &str) -> anyhow::Result<Vec<(String,)>> {
    sqlx::query_as(
        r#"SELECT ip FROM (
            SELECT ip_address AS ip FROM player_access_logs
            WHERE steam_id64 = $1 AND ip_address IS NOT NULL AND btrim(ip_address) <> ''
            UNION
            SELECT ip AS ip FROM server_online_players
            WHERE steam_id64 = $1 AND ip IS NOT NULL AND btrim(ip) <> ''
            UNION
            SELECT ip_address AS ip FROM ban_records
            WHERE steam_id = $1 AND ip_address IS NOT NULL AND btrim(ip_address) <> ''
        ) AS ips
        LIMIT 50"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn load_linked_accounts_by_ips(
    db: &Database,
    steamid64: &str,
    ips: &[String],
) -> anyhow::Result<Vec<IpUsageRow>> {
    if ips.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, IpUsageRow>(
        r#"SELECT ip, steam_id, player_name, max(last_seen_at) AS last_seen_at
           FROM (
             SELECT ip_address AS ip, steam_id64 AS steam_id, player_name, created_at AS last_seen_at
             FROM player_access_logs
             WHERE ip_address = ANY($1) AND steam_id64 <> $2
             UNION ALL
             SELECT ip AS ip, steam_id64 AS steam_id, name AS player_name, reported_at AS last_seen_at
             FROM server_online_players
             WHERE ip = ANY($1) AND steam_id64 <> $2
             UNION ALL
             SELECT ip_address AS ip, steam_id AS steam_id, player AS player_name, created_at AS last_seen_at
             FROM ban_records
             WHERE ip_address = ANY($1) AND steam_id <> $2
           ) AS raw
           WHERE steam_id ~ '^[0-9]{17}$'
           GROUP BY ip, steam_id, player_name
           ORDER BY max(last_seen_at) DESC NULLS LAST
           LIMIT 500"#,
    )
    .bind(ips)
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

async fn load_linked_signals(
    db: &Database,
    steamids: &[String],
) -> anyhow::Result<Vec<LinkedSignalRow>> {
    if steamids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, LinkedSignalRow>(
        r#"SELECT ids.steam_id,
                  COALESCE(wl.rejected_count, 0)::BIGINT AS rejected_whitelist_count
           FROM UNNEST($1::TEXT[]) AS ids(steam_id)
           LEFT JOIN (
             SELECT steamid64 AS steam_id, COUNT(*) AS rejected_count
             FROM whitelist_requests
             WHERE steamid64 = ANY($1) AND status = 'rejected'
             GROUP BY steamid64
           ) wl ON wl.steam_id = ids.steam_id"#,
    )
    .bind(steamids)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

/// 查询指定 Steam 绑定的 QQ openid（未绑定时返回 None）。
///
/// `steam_qq_bindings` 在 QQ 功能关闭的历史部署中可能不存在，
/// 此时按“未绑定”处理，避免风险评分整体失败。
async fn load_qq_openid_for_steam(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<String>> {
    match sqlx::query_as::<_, (String,)>(
        r#"SELECT qq_openid FROM steam_qq_bindings WHERE steamid64 = $1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    {
        Ok(row) => Ok(row.map(|(openid,)| openid)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("steam_qq_bindings") || msg.contains("does not exist") {
                Ok(None)
            } else {
                Err(e.into())
            }
        }
    }
}

/// 加载同 QQ openid 下绑定的其他 Steam 账号（含封禁/白名单拒绝信号）。
///
/// 同 QQ 账号数受 `max_bindings` 配置约束（默认 5，上限 50），结果集很小，
/// 因此可以直接批量查询封禁与拒绝信号，无需担心热点路径压力。
async fn load_qq_linked_accounts(
    db: &Database,
    qq_openid: &str,
    exclude_steamid64: &str,
) -> anyhow::Result<Vec<RiskLinkedAccount>> {
    let siblings: Vec<QqSiblingRow> = match sqlx::query_as::<_, QqSiblingRow>(
        r#"SELECT b.steamid64 AS steam_id, b.qq_openid, b.qq_username,
                  (SELECT nickname FROM whitelist_requests
                   WHERE steamid64 = b.steamid64
                   ORDER BY applied_at DESC LIMIT 1) AS whitelist_name,
                  b.verified_at
           FROM steam_qq_bindings b
           WHERE b.qq_openid = $1 AND b.steamid64 <> $2
           ORDER BY b.verified_at DESC
           LIMIT 50"#,
    )
    .bind(qq_openid)
    .bind(exclude_steamid64)
    .fetch_all(&db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            // 历史部署可能没有 steam_qq_bindings 表：按无 QQ 关联处理
            let msg = e.to_string();
            if msg.contains("steam_qq_bindings") || msg.contains("does not exist") {
                return Ok(Vec::new());
            }
            return Err(e.into());
        }
    };
    if siblings.is_empty() {
        return Ok(Vec::new());
    }

    let sibling_ids: Vec<String> = siblings.iter().map(|row| row.steam_id.clone()).collect();
    // 最近活跃时间：用于展示排序（QQ 关联本身是强身份关联，风险定级不依赖时间衰减）
    let activity_map: HashMap<String, Option<DateTime<Utc>>> =
        match sqlx::query_as::<_, QqActivityRow>(
            r#"SELECT steam_id, max(ts) AS last_seen FROM (
                    SELECT steam_id64 AS steam_id, created_at AS ts FROM player_access_logs
                    WHERE steam_id64 = ANY($1)
                    UNION ALL
                    SELECT steam_id64 AS steam_id, reported_at AS ts FROM server_online_players
                    WHERE steam_id64 = ANY($1)
                    UNION ALL
                    SELECT steam_id64 AS steam_id, COALESCE(left_at, last_seen_at) AS ts
                    FROM player_server_sessions WHERE steam_id64 = ANY($1)
                    UNION ALL
                    SELECT steam_id AS steam_id, created_at AS ts FROM ban_records
                    WHERE steam_id = ANY($1)
                ) AS times GROUP BY steam_id"#,
        )
        .bind(&sibling_ids)
        .fetch_all(&db.pool)
        .await
        {
            Ok(rows) => rows
                .into_iter()
                .map(|row| (row.steam_id, row.last_seen))
                .collect(),
            Err(_) => HashMap::new(),
        };

    let (sibling_bans, sibling_global_bans, sibling_signals) = tokio::try_join!(
        load_active_bans_for_steamids(db, &sibling_ids),
        load_active_global_bans(db, &sibling_ids),
        load_linked_signals(db, &sibling_ids),
    )?;
    let local_banned: HashSet<String> = sibling_bans
        .iter()
        .map(|row| row.steam_id.clone())
        .collect();
    let global_banned: HashSet<String> = sibling_global_bans
        .iter()
        .map(|row| row.steam_id64.clone())
        .collect();
    let signal_map: HashMap<String, LinkedSignalRow> = sibling_signals
        .into_iter()
        .map(|row| (row.steam_id.clone(), row))
        .collect();

    let mut accounts: Vec<RiskLinkedAccount> = siblings
        .into_iter()
        .map(|row| {
            let player_name = row
                .whitelist_name
                .clone()
                .or(row.qq_username.clone())
                .filter(|name| !name.trim().is_empty());
            let last_seen_at = activity_map
                .get(&row.steam_id)
                .copied()
                .flatten()
                .or(row.verified_at);
            RiskLinkedAccount {
                steamid64: row.steam_id.clone(),
                player_name,
                shared_ips: Vec::new(),
                last_seen_at,
                has_active_local_ban: local_banned.contains(&row.steam_id),
                has_active_global_ban: global_banned.contains(&row.steam_id),
                rejected_whitelist_count: signal_map
                    .get(&row.steam_id)
                    .map(|signal| signal.rejected_whitelist_count)
                    .unwrap_or(0),
                via_qq: true,
                qq_openid: Some(row.qq_openid),
            }
        })
        .collect();
    accounts.sort_by(|a, b| {
        linked_account_score(b)
            .cmp(&linked_account_score(a))
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
            .then_with(|| a.steamid64.cmp(&b.steamid64))
    });
    Ok(accounts)
}

/// 脱敏展示 QQ openid（风险原因文案用），只保留首尾各 4 个字符。
fn mask_qq_openid(openid: &str) -> String {
    let len = openid.chars().count();
    if len <= 8 {
        return "****".to_string();
    }
    let head: String = openid.chars().take(4).collect();
    let tail: String = openid.chars().skip(len - 4).collect();
    format!("{head}****{tail}")
}

fn add_self_ban_reasons(reasons: &mut Vec<RiskReason>, bans: &[ActiveBanRow]) {
    let synced_count = bans.iter().filter(|ban| ban.source == "global_ban").count();
    for ban in bans.iter().take(3) {
        reasons.push(RiskReason {
            code: "self_active_local_ban".to_string(),
            severity: RiskSeverity::Block,
            message: format!(
                "当前账号存在本地有效封禁：{}{}",
                ban.reason,
                ban.expires_at
                    .map(|dt| format!("，到期时间 {} (UTC)", dt.format("%Y-%m-%d %H:%M")))
                    .unwrap_or_else(|| "，永久封禁".to_string())
            ),
            steamid64: Some(ban.steam_id.clone()),
            ip: ban.ip_address.clone(),
            count: 1,
            last_seen_at: Some(ban.created_at),
        });
    }
    if synced_count > 0 {
        reasons.push(RiskReason {
            code: "self_synced_global_local_ban".to_string(),
            severity: RiskSeverity::Block,
            message: if synced_count > 1 {
                format!("当前账号存在 {synced_count} 条由全球封禁同步生成的本地封禁")
            } else {
                "当前账号存在由全球封禁同步生成的本地封禁".to_string()
            },
            steamid64: None,
            ip: None,
            count: synced_count as i64,
            last_seen_at: None,
        });
    }
}

fn add_self_global_ban_reasons(reasons: &mut Vec<RiskReason>, bans: &[GlobalBanRow]) {
    for ban in bans.iter().take(3) {
        reasons.push(RiskReason {
            code: "self_active_global_ban".to_string(),
            severity: RiskSeverity::Block,
            message: format!(
                "当前账号存在全球封禁：{}{}{}",
                ban.ban_type,
                ban.player_name
                    .as_deref()
                    .map(|name| format!(" / {name}"))
                    .unwrap_or_default(),
                ban.notes
                    .as_deref()
                    .map(|notes| format!("，备注：{notes}"))
                    .unwrap_or_default()
            ),
            steamid64: Some(ban.steam_id64.clone()),
            ip: None,
            count: 1,
            last_seen_at: ban.created_on.as_deref().and_then(parse_kzt_datetime),
        });
    }
}

fn add_linked_account_reasons(
    reasons: &mut Vec<RiskReason>,
    linked_accounts: &[RiskLinkedAccount],
) {
    let now = Utc::now();
    let linked_local_banned: Vec<&RiskLinkedAccount> = linked_accounts
        .iter()
        .filter(|account| account.has_active_local_ban)
        .collect();
    if !linked_local_banned.is_empty() {
        let recent_count = linked_local_banned
            .iter()
            .filter(|account| is_recent(account.last_seen_at, now, RECENT_IP_LINK_DAYS))
            .count();
        let severity = if recent_count > 0 {
            RiskSeverity::Block
        } else {
            RiskSeverity::Warning
        };
        reasons.push(RiskReason {
            code: "linked_ip_local_ban".to_string(),
            severity,
            message: format!(
                "同 IP 关联账号中有 {} 个存在本地有效封禁{}",
                linked_local_banned.len(),
                if recent_count > 0 {
                    format!("，其中 {recent_count} 个为近 {RECENT_IP_LINK_DAYS} 天关联")
                } else {
                    "，关联时间较早".to_string()
                }
            ),
            steamid64: linked_local_banned
                .first()
                .map(|account| account.steamid64.clone()),
            ip: linked_local_banned
                .first()
                .and_then(|account| account.shared_ips.first().cloned()),
            count: linked_local_banned.len() as i64,
            last_seen_at: linked_local_banned
                .iter()
                .filter_map(|account| account.last_seen_at)
                .max(),
        });
    }

    let linked_global_banned: Vec<&RiskLinkedAccount> = linked_accounts
        .iter()
        .filter(|account| account.has_active_global_ban)
        .collect();
    if !linked_global_banned.is_empty() {
        let recent_count = linked_global_banned
            .iter()
            .filter(|account| is_recent(account.last_seen_at, now, RECENT_IP_LINK_DAYS))
            .count();
        let severity = if recent_count > 0 {
            RiskSeverity::Block
        } else {
            RiskSeverity::Warning
        };
        reasons.push(RiskReason {
            code: "linked_ip_global_ban".to_string(),
            severity,
            message: format!(
                "同 IP 关联账号中有 {} 个存在全球封禁{}",
                linked_global_banned.len(),
                if recent_count > 0 {
                    format!("，其中 {recent_count} 个为近 {RECENT_IP_LINK_DAYS} 天关联")
                } else {
                    "，关联时间较早".to_string()
                }
            ),
            steamid64: linked_global_banned
                .first()
                .map(|account| account.steamid64.clone()),
            ip: linked_global_banned
                .first()
                .and_then(|account| account.shared_ips.first().cloned()),
            count: linked_global_banned.len() as i64,
            last_seen_at: linked_global_banned
                .iter()
                .filter_map(|account| account.last_seen_at)
                .max(),
        });
    }

    let rejected_count: i64 = linked_accounts
        .iter()
        .map(|account| account.rejected_whitelist_count)
        .sum();
    let total_negative = rejected_count;
    if total_negative > 0 {
        let latest = linked_accounts
            .iter()
            .filter(|account| account.rejected_whitelist_count > 0)
            .filter_map(|account| account.last_seen_at)
            .max();
        reasons.push(RiskReason {
            code: "linked_ip_negative_history".to_string(),
            severity: if is_recent(latest, now, OLD_IP_LINK_DAYS) {
                RiskSeverity::Block
            } else {
                RiskSeverity::Warning
            },
            message: format!("同 IP 关联账号存在负面历史：白名单申请被拒 {rejected_count} 次"),
            steamid64: None,
            ip: None,
            count: total_negative,
            last_seen_at: latest,
        });
    }

    if linked_accounts.len() > 5
        && reasons.iter().all(|reason| {
            reason.code != "linked_ip_local_ban"
                && reason.code != "linked_ip_global_ban"
                && reason.code != "linked_qq_local_ban"
                && reason.code != "linked_qq_global_ban"
        })
    {
        reasons.push(RiskReason {
            code: "many_linked_accounts".to_string(),
            severity: RiskSeverity::Info,
            message: format!(
                "该账号与 {} 个账号共享过 IP，请人工核对是否为公共网络",
                linked_accounts.len()
            ),
            steamid64: None,
            ip: None,
            count: linked_accounts.len() as i64,
            last_seen_at: linked_accounts
                .iter()
                .filter_map(|account| account.last_seen_at)
                .max(),
        });
    }
}

/// 同 QQ openid 关联风险：同一 QQ 下绑定的其他 Steam 账号。
///
/// QQ 绑定是强身份关联（换绑需管理员解绑），不按 IP 时间衰减：
/// 同 QQ 账号存在有效本地/全球封禁或白名单拒绝记录时，一律视为高风险（Block），
/// 要求强制通过并填写原因，同时进服时无白名单直接拦截（封禁类信号）。
fn add_qq_linked_account_reasons(
    reasons: &mut Vec<RiskReason>,
    qq_linked_accounts: &[RiskLinkedAccount],
) {
    if qq_linked_accounts.is_empty() {
        return;
    }
    let masked_openid = qq_linked_accounts
        .first()
        .and_then(|account| account.qq_openid.as_deref())
        .map(mask_qq_openid)
        .unwrap_or_else(|| "未知".to_string());

    let qq_local_banned: Vec<&RiskLinkedAccount> = qq_linked_accounts
        .iter()
        .filter(|account| account.has_active_local_ban)
        .collect();
    if !qq_local_banned.is_empty() {
        let names: Vec<String> = qq_local_banned
            .iter()
            .take(3)
            .map(|account| {
                account
                    .player_name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| account.steamid64.clone())
            })
            .collect();
        reasons.push(RiskReason {
            code: "linked_qq_local_ban".to_string(),
            severity: RiskSeverity::Block,
            message: format!(
                "同 QQ（{}）关联账号中有 {} 个存在本地有效封禁：{}",
                masked_openid,
                qq_local_banned.len(),
                names.join("、")
            ),
            steamid64: qq_local_banned
                .first()
                .map(|account| account.steamid64.clone()),
            ip: None,
            count: qq_local_banned.len() as i64,
            last_seen_at: qq_local_banned
                .iter()
                .filter_map(|account| account.last_seen_at)
                .max(),
        });
    }

    let qq_global_banned: Vec<&RiskLinkedAccount> = qq_linked_accounts
        .iter()
        .filter(|account| account.has_active_global_ban)
        .collect();
    if !qq_global_banned.is_empty() {
        let names: Vec<String> = qq_global_banned
            .iter()
            .take(3)
            .map(|account| {
                account
                    .player_name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| account.steamid64.clone())
            })
            .collect();
        reasons.push(RiskReason {
            code: "linked_qq_global_ban".to_string(),
            severity: RiskSeverity::Block,
            message: format!(
                "同 QQ（{}）关联账号中有 {} 个存在全球封禁：{}",
                masked_openid,
                qq_global_banned.len(),
                names.join("、")
            ),
            steamid64: qq_global_banned
                .first()
                .map(|account| account.steamid64.clone()),
            ip: None,
            count: qq_global_banned.len() as i64,
            last_seen_at: qq_global_banned
                .iter()
                .filter_map(|account| account.last_seen_at)
                .max(),
        });
    }

    let rejected_count: i64 = qq_linked_accounts
        .iter()
        .map(|account| account.rejected_whitelist_count)
        .sum();
    if rejected_count > 0 {
        let latest = qq_linked_accounts
            .iter()
            .filter(|account| account.rejected_whitelist_count > 0)
            .filter_map(|account| account.last_seen_at)
            .max();
        reasons.push(RiskReason {
            code: "linked_qq_negative_history".to_string(),
            severity: RiskSeverity::Block,
            message: format!(
                "同 QQ（{}）关联账号存在负面历史：白名单申请被拒 {rejected_count} 次",
                masked_openid
            ),
            steamid64: None,
            ip: None,
            count: rejected_count,
            last_seen_at: latest,
        });
    }
}

fn linked_account_score(account: &RiskLinkedAccount) -> i32 {
    let mut score = 0;
    if account.has_active_global_ban {
        score += 100;
    }
    if account.has_active_local_ban {
        score += 80;
    }
    score += (account.rejected_whitelist_count as i32).min(10) * 6;
    score
}

fn classify_action(reasons: &[RiskReason]) -> RiskAction {
    if reasons.iter().any(|reason| {
        matches!(
            reason.code.as_str(),
            "self_active_local_ban" | "self_synced_global_local_ban" | "self_active_global_ban"
        )
    }) {
        return RiskAction::Deny;
    }
    if reasons
        .iter()
        .any(|reason| reason.severity == RiskSeverity::Block)
    {
        return RiskAction::RequireForce;
    }
    if reasons
        .iter()
        .any(|reason| reason.severity == RiskSeverity::Warning)
    {
        return RiskAction::Warn;
    }
    RiskAction::Allow
}

fn classify_severity(reasons: &[RiskReason]) -> RiskSeverity {
    if reasons
        .iter()
        .any(|reason| reason.severity == RiskSeverity::Block)
    {
        RiskSeverity::Block
    } else if reasons
        .iter()
        .any(|reason| reason.severity == RiskSeverity::Warning)
    {
        RiskSeverity::Warning
    } else {
        RiskSeverity::Info
    }
}

fn risk_summary(
    action: &RiskAction,
    reasons: &[RiskReason],
    linked_account_count: usize,
    linked_banned_account_count: usize,
    linked_global_banned_account_count: usize,
    qq_linked_account_count: usize,
    qq_linked_banned_account_count: usize,
    qq_linked_global_banned_account_count: usize,
) -> String {
    if reasons.is_empty() {
        return "未发现本地封禁、全球封禁或同 IP / 同 QQ 高风险关联。".to_string();
    }
    match action {
        RiskAction::Deny => "当前账号存在有效本地/全球封禁，必须填写理由后强制通过。".to_string(),
        RiskAction::RequireForce => format!(
            "发现高风险关联：同 IP 账号 {linked_account_count} 个（本地封禁 {linked_banned_account_count} 个，全球封禁 {linked_global_banned_account_count} 个）；同 QQ 账号 {qq_linked_account_count} 个（本地封禁 {qq_linked_banned_account_count} 个，全球封禁 {qq_linked_global_banned_account_count} 个）。"
        ),
        RiskAction::Warn => "发现历史风险，需要管理员核对并填写备注。".to_string(),
        RiskAction::Allow => "仅发现低风险提示。".to_string(),
    }
}

fn risk_recommendation(action: &RiskAction) -> &'static str {
    match action {
        RiskAction::Deny => "默认拒绝通过；如确认需要放行，必须强制通过并填写原因。",
        RiskAction::RequireForce => {
            "默认拒绝通过；如确认是公共网络/误关联或同 QQ 号主正常换号，需强制通过并填写原因。"
        }
        RiskAction::Warn => "建议谨慎审核；如通过请填写审核备注。",
        RiskAction::Allow => "可以按正常流程审核。",
    }
}

fn is_recent(value: Option<DateTime<Utc>>, now: DateTime<Utc>, days: i64) -> bool {
    value
        .map(|dt| dt >= now - Duration::days(days))
        .unwrap_or(false)
}

fn parse_kzt_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(code: &str, severity: RiskSeverity) -> RiskReason {
        RiskReason {
            code: code.to_string(),
            severity,
            message: format!("测试信号 {code}"),
            steamid64: None,
            ip: None,
            count: 1,
            last_seen_at: None,
        }
    }

    #[test]
    fn ban_risk_reason_codes_stay_within_medium_or_high() {
        for code in BAN_RISK_REASON_CODES {
            assert!(is_ban_risk_reason_code(code), "未登记封禁类信号: {code}");
        }
        // 同 QQ 关联封禁同样属于封禁类信号，需要参与进服拦截
        assert!(is_ban_risk_reason_code("linked_qq_local_ban"));
        assert!(is_ban_risk_reason_code("linked_qq_global_ban"));
        // 非封禁类信号（白名单拒绝历史等）不得参与进服拦截
        assert!(!is_ban_risk_reason_code("linked_ip_negative_history"));
        assert!(!is_ban_risk_reason_code("linked_qq_negative_history"));
        assert!(!is_ban_risk_reason_code("many_linked_accounts"));
    }

    #[test]
    fn access_ban_risk_ignores_non_ban_signals() {
        assert!(access_ban_risk_from_reasons(Vec::new()).is_none());
        assert!(access_ban_risk_from_reasons(vec![reason(
            "linked_ip_negative_history",
            RiskSeverity::Block
        )])
        .is_none());
        assert!(access_ban_risk_from_reasons(vec![reason(
            "linked_qq_negative_history",
            RiskSeverity::Block
        )])
        .is_none());
        assert!(access_ban_risk_from_reasons(vec![reason(
            "many_linked_accounts",
            RiskSeverity::Info
        )])
        .is_none());
    }

    #[test]
    fn access_ban_risk_reports_linked_historical_ban_as_medium() {
        let risk = access_ban_risk_from_reasons(vec![reason(
            "linked_ip_local_ban",
            RiskSeverity::Warning,
        )])
        .expect("关联账号封禁应命中拦截");
        assert!(risk.is_medium_or_high());
        assert_eq!(risk.action, RiskAction::Warn);
        assert_eq!(risk.action.label(), "中风险");
        assert_eq!(risk.codes, vec!["linked_ip_local_ban".to_string()]);
        assert!(risk.detail.contains("linked_ip_local_ban"));
    }

    #[test]
    fn access_ban_risk_reports_self_ban_as_high() {
        let risk = access_ban_risk_from_reasons(vec![
            reason("self_active_global_ban", RiskSeverity::Block),
            reason("linked_ip_global_ban", RiskSeverity::Block),
        ])
        .expect("自身全球封禁应命中拦截");
        assert!(risk.is_medium_or_high());
        assert_eq!(risk.action, RiskAction::Deny);
        assert_eq!(risk.action.label(), "高风险");
        assert_eq!(
            risk.codes,
            vec![
                "self_active_global_ban".to_string(),
                "linked_ip_global_ban".to_string()
            ]
        );
    }

    fn qq_account(
        steamid64: &str,
        local_ban: bool,
        global_ban: bool,
        rejected: i64,
    ) -> RiskLinkedAccount {
        RiskLinkedAccount {
            steamid64: steamid64.to_string(),
            player_name: Some("同QQ玩家".to_string()),
            shared_ips: Vec::new(),
            last_seen_at: None,
            has_active_local_ban: local_ban,
            has_active_global_ban: global_ban,
            rejected_whitelist_count: rejected,
            via_qq: true,
            qq_openid: Some("qq-openid-test".to_string()),
        }
    }

    #[test]
    fn qq_linked_local_ban_is_high_risk_and_blocks_access() {
        let accounts = vec![qq_account("76561198000000011", true, false, 0)];
        let mut reasons = Vec::new();
        add_qq_linked_account_reasons(&mut reasons, &accounts);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].code, "linked_qq_local_ban");
        assert_eq!(reasons[0].severity, RiskSeverity::Block);
        assert!(reasons[0].message.contains("同 QQ"));

        let risk = access_ban_risk_from_reasons(reasons).expect("同 QQ 本地封禁应命中拦截");
        assert!(risk.is_medium_or_high());
        assert_eq!(risk.action, RiskAction::RequireForce);
        assert_eq!(risk.action.label(), "高风险");
        assert_eq!(risk.codes, vec!["linked_qq_local_ban".to_string()]);
    }

    #[test]
    fn qq_linked_global_ban_is_high_risk_and_blocks_access() {
        let accounts = vec![qq_account("76561198000000012", false, true, 0)];
        let mut reasons = Vec::new();
        add_qq_linked_account_reasons(&mut reasons, &accounts);
        assert_eq!(reasons[0].code, "linked_qq_global_ban");

        let risk = access_ban_risk_from_reasons(reasons).expect("同 QQ 全球封禁应命中拦截");
        assert_eq!(risk.action, RiskAction::RequireForce);
        assert_eq!(risk.codes, vec!["linked_qq_global_ban".to_string()]);
    }

    #[test]
    fn qq_linked_rejection_is_high_risk_but_not_access_blocking() {
        let accounts = vec![qq_account("76561198000000013", false, false, 2)];
        let mut reasons = Vec::new();
        add_qq_linked_account_reasons(&mut reasons, &accounts);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].code, "linked_qq_negative_history");
        // 白名单拒绝同样标记为高风险（Block），阻止自动通过与普通通过
        assert_eq!(reasons[0].severity, RiskSeverity::Block);
        assert_eq!(classify_action(&reasons), RiskAction::RequireForce);
        // 但进服只认封禁类信号，白名单拒绝不直接拦截进服
        assert!(access_ban_risk_from_reasons(reasons).is_none());
    }

    #[test]
    fn qq_linked_clean_accounts_produce_no_reasons() {
        let accounts = vec![qq_account("76561198000000014", false, false, 0)];
        let mut reasons = Vec::new();
        add_qq_linked_account_reasons(&mut reasons, &accounts);
        assert!(reasons.is_empty());
        let empty: Vec<RiskLinkedAccount> = Vec::new();
        let mut reasons = Vec::new();
        add_qq_linked_account_reasons(&mut reasons, &empty);
        assert!(reasons.is_empty());
    }

    #[test]
    fn mask_qq_openid_keeps_head_and_tail() {
        assert_eq!(mask_qq_openid("abc"), "****");
        assert_eq!(mask_qq_openid("1234567890ABCDEF"), "1234****CDEF");
    }

    #[test]
    fn risk_action_labels_match_admin_console_levels() {
        assert_eq!(RiskAction::Allow.label(), "低风险");
        assert_eq!(RiskAction::Warn.label(), "中风险");
        assert_eq!(RiskAction::RequireForce.label(), "高风险");
        assert_eq!(RiskAction::Deny.label(), "高风险");
    }
}
