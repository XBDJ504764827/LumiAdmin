use crate::{
    db::Database,
    services::{access_snapshot_service, ban_service::BanItem},
};
use chrono::{Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Convert Steam2 ID (STEAM_0:1:xxx) or Steam3 ID ([U:1:xxx]) to SteamID64
pub fn normalize_steam_id(steam_id: &str) -> String {
    let steam_id = steam_id.trim();

    // Already SteamID64 (17 digits starting with 7656119)
    if steam_id.len() == 17 && steam_id.starts_with("7656119") && steam_id.chars().all(|c| c.is_ascii_digit()) {
        return steam_id.to_string();
    }

    // Steam2 format: STEAM_X:Y:Z
    if steam_id.starts_with("STEAM_") {
        let parts: Vec<&str> = steam_id.split(':').collect();
        if parts.len() == 3 {
            if let (Ok(_), Ok(y), Ok(z)) = (
                parts[0].trim_start_matches("STEAM_").parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u64>(),
            ) {
                // SteamID64 = 76561197960265728 + Z * 2 + Y
                let steam_id64 = 76561197960265728u64 + z * 2 + y as u64;
                return steam_id64.to_string();
            }
        }
    }

    // Steam3 format: [U:1:xxx]
    if steam_id.starts_with("[U:1:") && steam_id.ends_with(']') {
        let id_str = &steam_id[5..steam_id.len()-1];
        if let Ok(id) = id_str.parse::<u64>() {
            let steam_id64 = 76561197960265728u64 + id;
            return steam_id64.to_string();
        }
    }

    // Return as-is if cannot parse
    steam_id.to_string()
}

#[derive(Debug, Clone)]
pub struct PluginBanInput {
    pub report_token: String,
    pub port: i32,
    pub ban_type: String,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub duration_minutes: i32,
    pub reason: String,
    pub operator_name: String,
}

#[derive(Debug, Clone)]
pub struct PluginUnbanInput {
    pub report_token: String,
    pub port: i32,
    pub target: String,
    pub reason: Option<String>,
    pub operator_name: String,
    pub operator_steamid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PluginBanCheckInput {
    pub report_token: String,
    pub port: i32,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub server_port: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct PluginBanPollInput {
    pub report_token: String,
    pub port: i32,
}

#[derive(Serialize)]
pub struct PluginBanCheckResult {
    pub banned: bool,
    pub reason: Option<String>,
    pub expires_at: Option<String>,
    pub message: String,
}

#[derive(sqlx::FromRow)]
pub struct ServerAuth {
    pub id: Uuid,
    pub name: String,
    pub port: i32,
}

impl ServerAuth {
    pub async fn authenticate(db: &Database, port: i32, report_token: &str) -> anyhow::Result<Self> {
        authenticate_server(db, port, report_token).await
    }
}


async fn authenticate_server(
    db: &Database,
    port: i32,
    report_token: &str,
) -> anyhow::Result<ServerAuth> {
    let token = report_token.trim();
    anyhow::ensure!(!token.is_empty(), "插件 token 不能为空");

    sqlx::query_as::<_, ServerAuth>(
        r#"SELECT id, name, port FROM servers WHERE port = $1 AND report_token = $2"#,
    )
    .bind(port)
    .bind(token)
    .fetch_one(&db.pool)
    .await
    .map_err(|_| anyhow::anyhow!("服务器 token 或端口无效"))
}


fn expires_at(duration_minutes: i32) -> Option<chrono::DateTime<Utc>> {
    if duration_minutes == 0 {
        None
    } else {
        Some(Utc::now() + Duration::minutes(i64::from(duration_minutes)))
    }
}

fn kick_message(reason: &str, expires_at: Option<&str>) -> String {
    match expires_at {
        Some(value) => {
            let formatted_time = format_expires_at_for_display(value);
            format!("你已被封禁，原因：{reason}，到期时间：{formatted_time}")
        }
        None => format!("你已被永久封禁，原因：{reason}"),
    }
}

fn format_expires_at_for_display(rfc3339_time: &str) -> String {
    // 尝试解析 RFC3339 时间并转换为本地时间显示格式
    match chrono::DateTime::parse_from_rfc3339(rfc3339_time) {
        Ok(datetime) => {
            // 转换为本地时区并格式化为易读格式：YYYY-MM-DD HH:MM:SS
            let local = datetime.with_timezone(&chrono::Local);
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        Err(_) => {
            // 解析失败时返回原始字符串
            rfc3339_time.to_string()
        }
    }
}


pub async fn create_plugin_ban(db: &Database, input: PluginBanInput) -> anyhow::Result<BanItem> {
    let server = authenticate_server(db, input.port, &input.report_token).await?;
    let ban_type = input.ban_type.trim();
    let reason = input.reason.trim();
    let operator_name = input.operator_name.trim();
    let steam_id = super::normalize_optional_string(input.steam_id.clone()).map(|s| normalize_steam_id(&s));
    let ip_address = super::normalize_optional_string(input.ip_address);

    anyhow::ensure!(matches!(ban_type, "steam" | "ip"), "封禁属性无效");
    anyhow::ensure!(input.duration_minutes >= 0, "封禁时长不能为负数");
    anyhow::ensure!(!reason.is_empty(), "封禁理由不能为空");
    anyhow::ensure!(!operator_name.is_empty(), "操作人不能为空");
    if ban_type == "steam" {
        anyhow::ensure!(steam_id.is_some(), "SteamID 不能为空");
    }
    if ban_type == "ip" {
        anyhow::ensure!(ip_address.is_some(), "IP 地址不能为空");
    }

    let duplicate_count: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (($1::TEXT IS NOT NULL AND steam_id = $1) OR ($2::TEXT IS NOT NULL AND ip_address = $2))"#,
    )
    .bind(steam_id.as_deref())
    .bind(ip_address.as_deref())
    .fetch_one(&db.pool)
    .await?;
    anyhow::ensure!(duplicate_count.0 == 0, "目标已有有效封禁");

    let expires_at = expires_at(input.duration_minutes);
    let row = sqlx::query_as::<_, super::ban_service::BanRow>(
        r#"INSERT INTO ban_records (
               id, player, steam_id, ip_address, server_name, ban_type,
               duration_minutes, expires_at, reason, status, operator_name, source,
               server_id, server_port
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'active', $10, 'game_plugin', $11, $12)
           RETURNING id, player, steam_id, ip_address, server_name, ban_type,
                     duration_minutes, expires_at, reason, status, operator_name, source,
                     server_id, server_port, removed_reason, removed_by, removed_at, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(super::normalize_optional_string(input.player))
    .bind(steam_id.unwrap_or_default())
    .bind(ip_address)
    .bind(server.name)
    .bind(ban_type)
    .bind(input.duration_minutes)
    .bind(expires_at)
    .bind(reason)
    .bind(operator_name)
    .bind(server.id)
    .bind(server.port)
    .fetch_one(&db.pool)
    .await?;

    Ok(super::ban_service::row_to_item(row))
}

pub async fn unban_plugin_target(
    db: &Database,
    input: PluginUnbanInput,
) -> anyhow::Result<BanItem> {
    authenticate_server(db, input.port, &input.report_token).await?;
    let target = input.target.trim();
    let normalized_target = normalize_steam_id(target);
    let operator_name = input.operator_name.trim();
    anyhow::ensure!(!target.is_empty(), "解封目标不能为空");
    anyhow::ensure!(!operator_name.is_empty(), "操作人不能为空");

    // 先查询封禁记录，获取原始封禁者
    let original_ban = sqlx::query_as::<_, super::ban_service::BanRow>(
        r#"SELECT id, player, steam_id, ip_address, server_name, ban_type,
                  duration_minutes, expires_at, reason, status, operator_name, source,
                  server_id, server_port, removed_reason, removed_by, removed_at, created_at
           FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (steam_id = $1 OR ip_address = $2)
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(&normalized_target)
    .bind(target)
    .fetch_optional(&db.pool)
    .await
    .map_err(|_| anyhow::anyhow!("查询封禁记录失败"))?;

    let Some(original_row) = original_ban else {
        anyhow::bail!("没有找到有效封禁");
    };

    // 检查权限：developer 和 admin 可以解封任何人，其他管理员只能解封自己封禁的
    let is_privileged = check_operator_privilege(db, input.operator_steamid.as_deref()).await?;
    if !is_privileged {
        let original_operator = original_row.operator_name.trim();
        if original_operator != operator_name {
            anyhow::bail!(
                "您无法解除其他管理员的封禁，请联系相关管理员"
            );
        }
    }

    // 执行解封
    let row = sqlx::query_as::<_, super::ban_service::BanRow>(
        r#"UPDATE ban_records
           SET status = 'inactive', removed_reason = $2, removed_by = $3, removed_at = now()
           WHERE id = $1
           RETURNING id, player, steam_id, ip_address, server_name, ban_type,
                     duration_minutes, expires_at, reason, status, operator_name, source,
                     server_id, server_port, removed_reason, removed_by, removed_at, created_at"#,
    )
    .bind(original_row.id)
    .bind(input.reason.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }))
    .bind(operator_name)
    .fetch_one(&db.pool)
    .await
    .map_err(|_| anyhow::anyhow!("解封失败"))?;

    Ok(super::ban_service::row_to_item(row))
}

/// 检查操作员是否具有特权（developer 或 admin）
async fn check_operator_privilege(db: &Database, operator_steamid: Option<&str>) -> anyhow::Result<bool> {
    let Some(steamid) = operator_steamid else {
        // 没有 SteamID，视为普通游戏内管理员
        return Ok(false);
    };

    let steamid = normalize_steam_id(steamid.trim());
    if steamid.is_empty() {
        return Ok(false);
    }

    // 查询用户角色
    let role: Option<(String,)> = sqlx::query_as(
        r#"SELECT role FROM users WHERE steam_id = $1 LIMIT 1"#,
    )
    .bind(&steamid)
    .fetch_optional(&db.pool)
    .await?;

    match role {
        Some((role,)) => {
            // developer 和 admin 具有特权
            Ok(role == "developer" || role == "admin")
        }
        None => {
            // 用户不存在于系统中，视为普通游戏内管理员
            Ok(false)
        }
    }
}

pub async fn poll_active_bans(
    db: &Database,
    input: PluginBanPollInput,
) -> anyhow::Result<Vec<BanItem>> {
    let server = authenticate_server(db, input.port, &input.report_token).await?;
    let rows = sqlx::query_as::<_, super::ban_service::BanRow>(
        r#"SELECT id, player, steam_id, ip_address, server_name, ban_type,
                  duration_minutes, expires_at, reason, status, operator_name, source,
                  server_id, server_port, removed_reason, removed_by, removed_at, created_at
           FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (server_id IS NULL OR server_id = $1 OR server_port = $2)
           ORDER BY created_at DESC
           LIMIT 10000"#,
    )
    .bind(server.id)
    .bind(server.port)
    .fetch_all(&db.pool)
    .await?;

    Ok(rows.into_iter().map(super::ban_service::row_to_item).collect())
}

/// 增量轮询封禁记录的结果
#[derive(Serialize)]
pub struct BanPollIncrementalResult {
    /// 封禁记录列表
    pub items: Vec<BanItem>,
    /// 当前游标（用于下次请求）
    pub cursor: String,
    /// 是否还有更多数据
    pub has_more: bool,
    /// 总活跃封禁数（仅首次请求时返回）
    pub total_count: Option<i64>,
}

/// 增量轮询活跃封禁记录
///
/// 使用游标机制实现增量同步：
/// - 首次请求不传 cursor，返回所有活跃封禁
/// - 后续请求传入上次返回的 cursor，仅返回新增的封禁记录
pub async fn poll_active_bans_incremental(
    db: &Database,
    input: PluginBanPollInput,
    cursor: Option<String>,
    limit: Option<i32>,
) -> anyhow::Result<BanPollIncrementalResult> {
    let server = authenticate_server(db, input.port, &input.report_token).await?;
    let limit = limit.unwrap_or(100).clamp(10, 500);

    // 解析游标（游标是 created_at 的时间戳）
    let since = match cursor {
        Some(c) => {
            // 游标格式：时间戳字符串
            match c.parse::<i64>() {
                Ok(ts) => Some(chrono::DateTime::from_timestamp(ts, 0)
                    .unwrap_or_else(chrono::Utc::now)),
                Err(_) => None,
            }
        }
        None => None,
    };

    // 构建查询
    let (rows, total_count) = if let Some(since_time) = since {
        // 增量查询：只获取新记录
        let rows = sqlx::query_as::<_, super::ban_service::BanRow>(
            r#"SELECT id, player, steam_id, ip_address, server_name, ban_type,
                      duration_minutes, expires_at, reason, status, operator_name, source,
                      server_id, server_port, removed_reason, removed_by, removed_at, created_at
               FROM ban_records
               WHERE status = 'active'
                 AND (expires_at IS NULL OR expires_at > now())
                 AND (server_id IS NULL OR server_id = $1 OR server_port = $2)
                 AND created_at > $3
               ORDER BY created_at ASC
               LIMIT $4"#,
        )
        .bind(server.id)
        .bind(server.port)
        .bind(since_time)
        .bind(limit)
        .fetch_all(&db.pool)
        .await?;

        let count: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM ban_records
               WHERE status = 'active'
                 AND (expires_at IS NULL OR expires_at > now())
                 AND (server_id IS NULL OR server_id = $1 OR server_port = $2)"#,
        )
        .bind(server.id)
        .bind(server.port)
        .fetch_one(&db.pool)
        .await?;

        (rows, count.0)
    } else {
        // 首次查询：获取所有活跃封禁
        let rows = sqlx::query_as::<_, super::ban_service::BanRow>(
            r#"SELECT id, player, steam_id, ip_address, server_name, ban_type,
                      duration_minutes, expires_at, reason, status, operator_name, source,
                      server_id, server_port, removed_reason, removed_by, removed_at, created_at
               FROM ban_records
               WHERE status = 'active'
                 AND (expires_at IS NULL OR expires_at > now())
                 AND (server_id IS NULL OR server_id = $1 OR server_port = $2)
               ORDER BY created_at DESC
               LIMIT $3"#,
        )
        .bind(server.id)
        .bind(server.port)
        .bind(limit)
        .fetch_all(&db.pool)
        .await?;

        let count: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM ban_records
               WHERE status = 'active'
                 AND (expires_at IS NULL OR expires_at > now())
                 AND (server_id IS NULL OR server_id = $1 OR server_port = $2)"#,
        )
        .bind(server.id)
        .bind(server.port)
        .fetch_one(&db.pool)
        .await?;

        (rows, count.0)
    };

    // 计算新的游标（最后一条记录的 created_at 时间戳）
    let new_cursor = rows.last()
        .map(|row| row.created_at.timestamp().to_string())
        .unwrap_or_else(|| chrono::Utc::now().timestamp().to_string());

    let has_more = rows.len() as i32 == limit;

    Ok(BanPollIncrementalResult {
        items: rows.into_iter().map(super::ban_service::row_to_item).collect(),
        cursor: new_cursor,
        has_more,
        total_count: Some(total_count),
    })
}

pub async fn check_plugin_ban(
    db: &Database,
    snapshot_store: &access_snapshot_service::SnapshotStore,
    input: PluginBanCheckInput,
) -> anyhow::Result<PluginBanCheckResult> {
    let steam_id = super::normalize_optional_string(input.steam_id.clone());
    let ip_address = super::normalize_optional_string(input.ip_address.clone());
    anyhow::ensure!(
        steam_id.is_some() || ip_address.is_some(),
        "SteamID 或 IP 不能为空"
    );

    match check_plugin_ban_live(db, input.clone()).await {
        Ok(result) => Ok(result),
        Err(error) => {
            tracing::warn!(%error, "live plugin ban check failed, trying snapshot fallback");
            let Some(snapshot) = snapshot_store.read_snapshot().await? else {
                anyhow::bail!("访问控制服务暂时不可用");
            };
            let decision = access_snapshot_service::evaluate_ban_snapshot(
                &snapshot,
                &access_snapshot_service::SnapshotBanInput {
                    report_token: input.report_token,
                    port: input.port,
                    steam_id64: steam_id,
                    ip_address,
                    now: Utc::now(),
                },
            );
            if !decision.available {
                anyhow::bail!("访问控制服务暂时不可用");
            }
            Ok(PluginBanCheckResult {
                banned: decision.banned,
                reason: decision.reason.clone(),
                expires_at: decision.expires_at.map(|t| t.to_rfc3339()),
                message: decision
                    .reason
                    .as_deref()
                    .map(|reason| kick_message(reason, decision.expires_at.as_ref().map(|t| t.to_rfc3339()).as_deref()))
                    .unwrap_or_else(|| "未封禁".to_string()),
            })
        }
    }
}

async fn check_plugin_ban_live(
    db: &Database,
    input: PluginBanCheckInput,
) -> anyhow::Result<PluginBanCheckResult> {
    let server = authenticate_server(db, input.port, &input.report_token).await?;
    let steam_id = super::normalize_optional_string(input.steam_id);
    let ip_address = super::normalize_optional_string(input.ip_address);
    let player = super::normalize_optional_string(input.player);
    let server_port = input.server_port.unwrap_or(server.port);
    anyhow::ensure!(
        steam_id.is_some() || ip_address.is_some(),
        "SteamID 或 IP 不能为空"
    );

    let row = sqlx::query_as::<_, (Uuid, String, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT id, reason, expires_at FROM ban_records
           WHERE status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
             AND (($1::TEXT IS NOT NULL AND steam_id = $1) OR ($2::TEXT IS NOT NULL AND ip_address = $2))
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(steam_id.as_deref())
    .bind(ip_address.as_deref())
    .fetch_optional(&db.pool)
    .await?;

    if let Some((ban_id, reason, expires_at)) = row {
        complete_missing_ban_details(
            db,
            ban_id,
            player.as_deref(),
            ip_address.as_deref(),
            &server,
            server_port,
        )
        .await?;
        let expires = expires_at.map(|value| value.to_rfc3339());
        return Ok(PluginBanCheckResult {
            banned: true,
            reason: Some(reason.clone()),
            expires_at: expires.clone(),
            message: kick_message(&reason, expires.as_deref()),
        });
    }

    Ok(PluginBanCheckResult {
        banned: false,
        reason: None,
        expires_at: None,
        message: "未封禁".to_string(),
    })
}

pub async fn complete_missing_ban_details(
    db: &Database,
    ban_id: Uuid,
    player: Option<&str>,
    ip_address: Option<&str>,
    server: &ServerAuth,
    server_port: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE ban_records
           SET player = COALESCE(player, $2),
               ip_address = COALESCE(ip_address, $3),
               server_name = COALESCE(server_name, $4),
               server_id = COALESCE(server_id, $5),
               server_port = COALESCE(server_port, $6)
           WHERE id = $1"#,
    )
    .bind(ban_id)
    .bind(player)
    .bind(ip_address)
    .bind(&server.name)
    .bind(server.id)
    .bind(server_port)
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{expires_at, format_expires_at_for_display, kick_message, normalize_steam_id};

    #[test]
    fn zero_duration_has_no_expiry() {
        assert!(expires_at(0).is_none());
    }

    #[test]
    fn positive_duration_has_expiry() {
        assert!(expires_at(30).is_some());
    }

    #[test]
    fn kick_message_mentions_permanent_ban() {
        assert_eq!(kick_message("作弊", None), "你已被永久封禁，原因：作弊");
    }

    #[test]
    fn kick_message_formats_expiry_time_in_local_timezone() {
        // UTC 时间：2024-01-15T10:30:00Z
        let utc_time = "2024-01-15T10:30:00+00:00";
        let message = kick_message("作弊", Some(utc_time));
        // 消息应该包含本地时间格式，而不是原始的 RFC3339 格式
        assert!(message.contains("到期时间："));
        assert!(!message.contains("T10:30:00+00:00")); // 不应该包含原始格式
    }

    #[test]
    fn format_expires_at_converts_rfc3339_to_local_display() {
        // UTC 时间：2024-01-15T10:30:00Z
        let utc_time = "2024-01-15T10:30:00+00:00";
        let formatted = format_expires_at_for_display(utc_time);
        // 格式应该是 YYYY-MM-DD HH:MM:SS
        assert!(formatted.contains("-"));
        assert!(formatted.contains(" "));
        assert!(formatted.contains(":"));
        // 不应该包含时区信息
        assert!(!formatted.contains("+"));
        assert!(!formatted.contains("T"));
    }

    #[test]
    fn format_expires_at_handles_invalid_input_gracefully() {
        let invalid = "not-a-valid-time";
        let formatted = format_expires_at_for_display(invalid);
        assert_eq!(formatted, invalid);
    }

    #[test]
    fn normalize_steam_id_converts_steam2() {
        // STEAM_0:1:12345 -> 76561197960265728 + 12345 * 2 + 1 = 76561197960290419
        assert_eq!(normalize_steam_id("STEAM_0:1:12345"), "76561197960290419");
        assert_eq!(normalize_steam_id("STEAM_1:0:54321"), "76561197960374370"); // universe bit ignored
    }

    #[test]
    fn normalize_steam_id_keeps_steamid64() {
        assert_eq!(normalize_steam_id("76561197960265728"), "76561197960265728");
        assert_eq!(normalize_steam_id("76561198123456789"), "76561198123456789");
    }

    #[test]
    fn normalize_steam_id_handles_whitespace() {
        assert_eq!(normalize_steam_id("  76561197960265728  "), "76561197960265728");
    }
}
