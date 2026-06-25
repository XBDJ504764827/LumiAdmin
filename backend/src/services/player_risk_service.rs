use crate::db::Database;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const RECENT_IP_LINK_DAYS: i64 = 90;
const OLD_IP_LINK_DAYS: i64 = 365;
const MAX_LINKED_ACCOUNT_ITEMS: usize = 20;

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
    pub upheld_report_count: i64,
    pub failed_appeal_count: i64,
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
    upheld_report_count: i64,
    failed_appeal_count: i64,
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
    let (self_bans, self_global_bans, player_ip_rows) = tokio::try_join!(
        load_active_bans_for_steamids(db, &self_steamids),
        load_active_global_bans(db, &self_steamids),
        load_player_ips(db, steamid64),
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
                upheld_report_count: 0,
                failed_appeal_count: 0,
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
            account.upheld_report_count = signal.upheld_report_count;
            account.failed_appeal_count = signal.failed_appeal_count;
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

    let mut reasons = Vec::new();
    add_self_ban_reasons(&mut reasons, &self_bans);
    add_self_global_ban_reasons(&mut reasons, &self_global_bans);
    add_linked_account_reasons(&mut reasons, &linked_accounts);

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
    let summary = risk_summary(
        &action,
        &reasons,
        linked_account_count,
        linked_banned_account_count,
        linked_global_banned_account_count,
    );
    let recommendation = risk_recommendation(&action).to_string();

    linked_accounts.truncate(MAX_LINKED_ACCOUNT_ITEMS);

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
    })
}

pub async fn evaluate_ip_ban_for_access(
    db: &Database,
    steamid64: &str,
    ip_address: Option<&str>,
) -> anyhow::Result<Option<RiskReason>> {
    let Some(ip_address) = ip_address.map(str::trim).filter(|ip| !ip.is_empty()) else {
        return Ok(None);
    };
    let steamid64 = steamid64.trim();
    let row: Option<(String, Option<String>, bool, bool, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT linked.steam_id,
                  linked.player_name,
                  EXISTS (
                    SELECT 1 FROM ban_records br
                    WHERE br.steam_id = linked.steam_id
                      AND br.status = 'active'
                      AND (br.expires_at IS NULL OR br.expires_at > now())
                  ) AS has_local_ban,
                  EXISTS (
                    SELECT 1 FROM global_bans gb
                    WHERE gb.steam_id64 = linked.steam_id
                      AND gb.is_expired = false
                      AND gb.manual_unbanned = false
                  ) AS has_global_ban,
                  linked.last_seen_at
           FROM (
             SELECT steam_id, max(player_name) AS player_name, max(last_seen_at) AS last_seen_at
             FROM (
               SELECT steam_id64 AS steam_id, player_name, created_at AS last_seen_at
               FROM player_access_logs
               WHERE ip_address = $1 AND steam_id64 <> $2
               UNION ALL
               SELECT steam_id64 AS steam_id, name AS player_name, reported_at AS last_seen_at
               FROM server_online_players
               WHERE ip = $1 AND steam_id64 <> $2
               UNION ALL
               SELECT steam_id AS steam_id, player AS player_name, created_at AS last_seen_at
               FROM ban_records
               WHERE ip_address = $1 AND steam_id <> $2
             ) AS raw
             GROUP BY steam_id
           ) linked
           WHERE EXISTS (
             SELECT 1 FROM ban_records br
             WHERE br.steam_id = linked.steam_id
               AND br.status = 'active'
               AND (br.expires_at IS NULL OR br.expires_at > now())
           )
           OR EXISTS (
             SELECT 1 FROM global_bans gb
             WHERE gb.steam_id64 = linked.steam_id
               AND gb.is_expired = false
               AND gb.manual_unbanned = false
           )
           ORDER BY has_global_ban DESC, has_local_ban DESC, linked.last_seen_at DESC NULLS LAST
           LIMIT 1"#,
    )
    .bind(ip_address)
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await?;

    Ok(row.map(
        |(linked_steamid, player_name, has_local_ban, has_global_ban, last_seen_at)| {
            let message = if has_global_ban {
                format!(
                    "同 IP 关联账号 {}{} 存在全球封禁",
                    player_name
                        .as_deref()
                        .map(|name| format!("{name} / "))
                        .unwrap_or_default(),
                    linked_steamid
                )
            } else if has_local_ban {
                format!(
                    "同 IP 关联账号 {}{} 存在本地有效封禁",
                    player_name
                        .as_deref()
                        .map(|name| format!("{name} / "))
                        .unwrap_or_default(),
                    linked_steamid
                )
            } else {
                format!("同 IP 关联账号 {linked_steamid} 存在封禁风险")
            };
            RiskReason {
                code: if has_global_ban {
                    "linked_ip_global_ban".to_string()
                } else {
                    "linked_ip_local_ban".to_string()
                },
                severity: RiskSeverity::Block,
                message,
                steamid64: Some(linked_steamid),
                ip: Some(ip_address.to_string()),
                count: 1,
                last_seen_at,
            }
        },
    ))
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
                  COALESCE(wl.rejected_count, 0)::BIGINT AS rejected_whitelist_count,
                  COALESCE(reports.upheld_count, 0)::BIGINT AS upheld_report_count,
                  COALESCE(appeals.failed_count, 0)::BIGINT AS failed_appeal_count
           FROM UNNEST($1::TEXT[]) AS ids(steam_id)
           LEFT JOIN (
             SELECT steamid64 AS steam_id, COUNT(*) AS rejected_count
             FROM whitelist_requests
             WHERE steamid64 = ANY($1) AND status = 'rejected'
             GROUP BY steamid64
           ) wl ON wl.steam_id = ids.steam_id
           LEFT JOIN (
             SELECT target_steam_id AS steam_id, COUNT(*) AS upheld_count
             FROM player_reports
             WHERE target_steam_id = ANY($1) AND status = 'approved'
             GROUP BY target_steam_id
           ) reports ON reports.steam_id = ids.steam_id
           LEFT JOIN (
             SELECT steam_id, COUNT(*) AS failed_count
             FROM ban_appeals
             WHERE steam_id = ANY($1) AND status = 'rejected'
             GROUP BY steam_id
           ) appeals ON appeals.steam_id = ids.steam_id"#,
    )
    .bind(steamids)
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}

fn add_self_ban_reasons(reasons: &mut Vec<RiskReason>, bans: &[ActiveBanRow]) {
    for ban in bans.iter().take(3) {
        reasons.push(RiskReason {
            code: "self_active_local_ban".to_string(),
            severity: RiskSeverity::Block,
            message: format!(
                "当前账号存在本地有效封禁：{}{}",
                ban.reason,
                ban.expires_at
                    .map(|dt| format!("，到期时间 {}", dt.to_rfc3339()))
                    .unwrap_or_else(|| "，永久封禁".to_string())
            ),
            steamid64: Some(ban.steam_id.clone()),
            ip: ban.ip_address.clone(),
            count: 1,
            last_seen_at: Some(ban.created_at),
        });
        if ban.source == "global_ban" {
            reasons.push(RiskReason {
                code: "self_synced_global_local_ban".to_string(),
                severity: RiskSeverity::Block,
                message: "当前账号存在由全球封禁同步生成的本地封禁".to_string(),
                steamid64: Some(ban.steam_id.clone()),
                ip: ban.ip_address.clone(),
                count: 1,
                last_seen_at: Some(ban.created_at),
            });
        }
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
    let upheld_report_count: i64 = linked_accounts
        .iter()
        .map(|account| account.upheld_report_count)
        .sum();
    let failed_appeal_count: i64 = linked_accounts
        .iter()
        .map(|account| account.failed_appeal_count)
        .sum();
    let total_negative = rejected_count + upheld_report_count + failed_appeal_count;
    if total_negative > 0 {
        let latest = linked_accounts
            .iter()
            .filter(|account| {
                account.rejected_whitelist_count > 0
                    || account.upheld_report_count > 0
                    || account.failed_appeal_count > 0
            })
            .filter_map(|account| account.last_seen_at)
            .max();
        reasons.push(RiskReason {
            code: "linked_ip_negative_history".to_string(),
            severity: if is_recent(latest, now, OLD_IP_LINK_DAYS) {
                RiskSeverity::Block
            } else {
                RiskSeverity::Warning
            },
            message: format!(
                "同 IP 关联账号存在负面历史：白名单拒绝 {rejected_count} 次，举报成立 {upheld_report_count} 次，申诉失败 {failed_appeal_count} 次"
            ),
            steamid64: None,
            ip: None,
            count: total_negative,
            last_seen_at: latest,
        });
    }

    if linked_accounts.len() > 5
        && reasons.iter().all(|reason| {
            reason.code != "linked_ip_local_ban" && reason.code != "linked_ip_global_ban"
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

fn linked_account_score(account: &RiskLinkedAccount) -> i32 {
    let mut score = 0;
    if account.has_active_global_ban {
        score += 100;
    }
    if account.has_active_local_ban {
        score += 80;
    }
    score += (account.rejected_whitelist_count as i32).min(10) * 6;
    score += (account.upheld_report_count as i32).min(10) * 8;
    score += (account.failed_appeal_count as i32).min(10) * 4;
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
) -> String {
    if reasons.is_empty() {
        return "未发现本地封禁、全球封禁或同 IP 高风险关联。".to_string();
    }
    match action {
        RiskAction::Deny => "当前账号存在有效本地/全球封禁，必须填写理由后强制通过。".to_string(),
        RiskAction::RequireForce => format!(
            "发现高风险关联：同 IP 账号 {linked_account_count} 个，本地封禁 {linked_banned_account_count} 个，全球封禁 {linked_global_banned_account_count} 个。"
        ),
        RiskAction::Warn => "发现历史风险，需要管理员核对并填写备注。".to_string(),
        RiskAction::Allow => "仅发现低风险提示。".to_string(),
    }
}

fn risk_recommendation(action: &RiskAction) -> &'static str {
    match action {
        RiskAction::Deny => "默认拒绝通过；如确认需要放行，必须强制通过并填写原因。",
        RiskAction::RequireForce => {
            "默认拒绝通过；如确认是公共网络或误关联，需强制通过并填写原因。"
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
