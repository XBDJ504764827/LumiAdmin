//! 插件免配置自识别服务。
//!
//! 游戏服插件只需在 `core.cfg` 中填写面板地址，即可在启动时调用
//! `POST /api/plugin/identify` 自动领取本服的 `report_token`，无需逐服复制 token。
//!
//! 识别优先级：
//! 1. 已绑定的安装实例（`plugin_instance_id` + 端口）——IP 变化后可继续定位；
//! 2. 请求来源 IP + 端口精确匹配面板中登记的服务器；
//! 3. 来源 IP 为空/unknown 且端口唯一时的宽松匹配（仅限从未绑定过的服务器）。
//!
//! 安全边界：来源 IP 由 TCP 连接（或在可信代理场景下由转发头）确定，
//! 攻击者无法凭自己的 IP 认领他人服务器；可选的面板级安装密钥提供额外保护。

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::db::Database;

const MAX_INSTALL_ID_LEN: usize = 128;

#[derive(Debug, Clone)]
pub struct IdentifyInput {
    pub port: i32,
    pub hostname: Option<String>,
    pub game: Option<String>,
    pub install_id: Option<String>,
    pub client_ip: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentifyResult {
    pub server_id: Uuid,
    pub server_name: String,
    pub community_name: Option<String>,
    pub report_token: String,
    /// 本次是否新建/更新了绑定关系
    pub bound: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct IdentifyRow {
    id: Uuid,
    name: String,
    report_token: String,
    plugin_instance_id: Option<String>,
    plugin_last_seen_at: Option<DateTime<Utc>>,
    community_name: Option<String>,
}

const SELECT_COLUMNS: &str = r#"
    SELECT s.id,
           s.name,
           s.report_token,
           s.plugin_instance_id,
           s.plugin_last_seen_at,
           c.name AS community_name
      FROM servers s
      LEFT JOIN communities c ON c.id = s.community_id
"#;

/// 插件自识别主入口。
pub async fn identify(
    db: &Database,
    input: &IdentifyInput,
    auto_bind: bool,
    rebind_after_secs: i64,
) -> anyhow::Result<IdentifyResult> {
    anyhow::ensure!(
        (1..=65535).contains(&input.port),
        "端口无效：{}",
        input.port
    );

    let install_id = normalize_install_id(input.install_id.as_deref());
    let client_ip = normalize_client_ip(&input.client_ip);

    tracing::debug!(
        port = input.port,
        client_ip = %client_ip,
        hostname = ?input.hostname,
        game = ?input.game,
        install_id = ?install_id,
        "插件自识别请求"
    );

    // hostname 自动跟随：identify 成功后把游戏服上报的 hostname 回写为 servers.name
    //（允许中文/空格/特殊符号，仅截断到 128 字符；改名即跟随，不做唯一性约束）。
    // 具体回写在命中服务器后执行（touch_with_hostname），此处仅规范化。
    let reported_hostname = input
        .hostname
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| {
            let mut owned = v.to_string();
            if owned.chars().count() > 128 {
                owned = owned.chars().take(128).collect();
            }
            owned
        });

    // 1) 已绑定的安装实例优先（容忍来源 IP 变化）
    if let Some(ref instance_id) = install_id {
        let row: Option<IdentifyRow> = sqlx::query_as(&format!(
            "{SELECT_COLUMNS} WHERE s.plugin_instance_id = $1 AND s.port = $2 LIMIT 1"
        ))
        .bind(instance_id)
        .bind(input.port)
        .fetch_optional(&db.pool)
        .await?;

        if let Some(row) = row {
            touch_with_hostname(db, row.id, Some(instance_id), reported_hostname.as_deref())
                .await?;
            tracing::info!(
                server_id = %row.id,
                port = input.port,
                install_id = %instance_id,
                "插件自识别：命中已绑定安装实例"
            );
            return Ok(into_result(row, false));
        }
    }

    if !auto_bind {
        anyhow::bail!("未找到已绑定的服务器，且面板已关闭插件自动绑定（PLUGIN_AUTO_BIND=false）");
    }

    anyhow::ensure!(
        !client_ip.is_empty() && client_ip != "unknown",
        "无法确定请求来源 IP，请为插件配置 report_token 或开启可信代理头"
    );

    // 2) 来源 IP + 端口精确匹配
    let rows: Vec<IdentifyRow> = sqlx::query_as(&format!(
        "{SELECT_COLUMNS} WHERE s.port = $1 AND s.ip = $2 ORDER BY s.created_at ASC"
    ))
    .bind(input.port)
    .bind(&client_ip)
    .fetch_all(&db.pool)
    .await?;

    match rows.len() {
        1 => {
            let row = rows.into_iter().next().expect("checked len == 1");
            bind_or_reject(db, row, install_id.as_deref(), rebind_after_secs, reported_hostname.as_deref()).await
        }
        0 => {
            // 3) 面板未填写 IP（空/unknown）时的宽松匹配：仅限唯一且未绑定的服务器
            let fallback: Vec<IdentifyRow> = sqlx::query_as(&format!(
                "{SELECT_COLUMNS} WHERE s.port = $1 \
                 AND (s.ip IS NULL OR btrim(s.ip) = '' OR lower(btrim(s.ip)) = 'unknown') \
                 AND s.plugin_instance_id IS NULL \
                 ORDER BY s.created_at ASC"
            ))
            .bind(input.port)
            .fetch_all(&db.pool)
            .await?;

            if fallback.len() == 1 {
                let row = fallback.into_iter().next().expect("checked len == 1");
                tracing::warn!(
                    server_id = %row.id,
                    port = input.port,
                    client_ip = %client_ip,
                    "插件自识别：服务器未填写 IP，使用端口宽松匹配（建议在面板补全服务器 IP）"
                );
                bind_or_reject(db, row, install_id.as_deref(), rebind_after_secs, reported_hostname.as_deref()).await
            } else {
                anyhow::bail!(
                    "未找到与来源地址 {}:{} 匹配的服务器，请在面板中新增该服务器并填写正确的公网 IP 与端口",
                    client_ip,
                    input.port
                );
            }
        }
        _ => anyhow::bail!(
            "来源地址 {}:{} 匹配到多台服务器，请为这些服务器设置不同端口，或在 core.cfg 中手动填写 report_token",
            client_ip,
            input.port
        ),
    }
}

async fn bind_or_reject(
    db: &Database,
    row: IdentifyRow,
    install_id: Option<&str>,
    rebind_after_secs: i64,
    reported_hostname: Option<&str>,
) -> anyhow::Result<IdentifyResult> {
    let now = Utc::now();
    let existing = row.plugin_instance_id.as_deref();

    let same_instance = match (existing, install_id) {
        (Some(a), Some(b)) => a == b,
        (None, _) => true,
        (Some(_), None) => false,
    };

    let stale = row
        .plugin_last_seen_at
        .map(|seen| now - seen > Duration::seconds(rebind_after_secs.max(0)))
        .unwrap_or(true);

    if !same_instance && !stale {
        anyhow::bail!(
            "服务器「{}」已被另一个插件实例绑定，且最近仍在活动。如确需更换，请在面板中重置该服务器的绑定",
            row.name
        );
    }

    touch_with_hostname(db, row.id, install_id, reported_hostname).await?;
    tracing::info!(
        server_id = %row.id,
        server_name = %row.name,
        install_id = ?install_id,
        "插件自识别：已绑定服务器"
    );
    Ok(into_result(row, true))
}

async fn touch_with_hostname(
    db: &Database,
    server_id: Uuid,
    install_id: Option<&str>,
    reported_hostname: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE servers
              SET plugin_instance_id = COALESCE($2, plugin_instance_id),
                  plugin_last_seen_at = now(),
                  plugin_bound_at = COALESCE(plugin_bound_at, now()),
                  name = COALESCE($3, name)
            WHERE id = $1"#,
    )
    .bind(server_id)
    .bind(install_id)
    .bind(reported_hostname)
    .execute(&db.pool)
    .await?;
    Ok(())
}

fn normalize_install_id(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|v| !v.is_empty()).map(|v| {
        if v.len() > MAX_INSTALL_ID_LEN {
            v[..MAX_INSTALL_ID_LEN].to_string()
        } else {
            v.to_string()
        }
    })
}

/// 将 IPv6 映射地址（::ffff:1.2.3.4）归一化为 IPv4，便于与面板登记地址比较。
pub fn normalize_client_ip(ip: &str) -> String {
    let trimmed = ip.trim();
    if let Some(rest) = trimmed.strip_prefix("::ffff:") {
        return rest.to_string();
    }
    trimmed.to_string()
}

fn into_result(row: IdentifyRow, bound: bool) -> IdentifyResult {
    IdentifyResult {
        server_id: row.id,
        server_name: row.name,
        community_name: row.community_name,
        report_token: row.report_token,
        bound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ipv4_mapped_address() {
        assert_eq!(normalize_client_ip("::ffff:203.0.113.7"), "203.0.113.7");
        assert_eq!(normalize_client_ip(" 203.0.113.7 "), "203.0.113.7");
        assert_eq!(normalize_client_ip("2001:db8::1"), "2001:db8::1");
    }

    #[test]
    fn trims_and_caps_install_id() {
        assert_eq!(normalize_install_id(Some("  ")), None);
        assert_eq!(normalize_install_id(Some(" abc ")).as_deref(), Some("abc"));
        let long = "x".repeat(500);
        assert_eq!(normalize_install_id(Some(&long)).unwrap().len(), 128);
    }
}
