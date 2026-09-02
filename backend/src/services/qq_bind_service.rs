//! QQ 群绑定服务
//!
//! 玩家通过 Steam 验证后，网站生成一次性绑定 UUID；玩家把 UUID 发到
//! QQ 群 @机器人，机器人在群消息中获取发送者 openid，调用本服务接口
//! 完成 steamid64 ↔ qq_openid 绑定。此后玩家提交白名单申请时，网站
//! 自动使用绑定的 QQ openid 作为联系方式（contact = qq:<openid>），
//! 避免手动填写错误。

use crate::db::Database;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// 绑定码有效期（分钟）
pub const BIND_CODE_TTL_MINUTES: i64 = 10;

/// 绑定码状态
pub const CODE_PENDING: &str = "pending";
#[allow(dead_code)]
pub const CODE_BOUND: &str = "bound";
#[allow(dead_code)]
pub const CODE_EXPIRED: &str = "expired";
#[allow(dead_code)]
pub const CODE_REVOKED: &str = "revoked";

/// 生成一次性绑定码：为已验证 Steam 的玩家创建待绑定码。
///
/// 一个玩家同时只允许一个 pending 码（旧码自动撤销，避免堆积）。
pub async fn create_bind_code(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<(Uuid, DateTime<Utc>)> {
    let code = Uuid::new_v4();
    let expires_at = Utc::now() + chrono::Duration::minutes(BIND_CODE_TTL_MINUTES);

    let mut tx = db.pool.begin().await?;
    // 撤销该玩家旧的 pending 码，确保同时只有一个有效码
    sqlx::query(
        r#"UPDATE qq_bind_codes SET status = 'revoked'
           WHERE steamid64 = $1 AND status = 'pending'"#,
    )
    .bind(steamid64)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO qq_bind_codes (id, steamid64, status, expires_at)
           VALUES ($1, $2, 'pending', $3)"#,
    )
    .bind(code)
    .bind(steamid64)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((code, expires_at))
}

/// 查询玩家当前绑定码状态（用于申请页展示）。
#[allow(clippy::type_complexity)]
pub async fn get_pending_code(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<(Uuid, DateTime<Utc>)>> {
    let row: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT id, expires_at FROM qq_bind_codes
           WHERE steamid64 = $1 AND status = 'pending'
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await?;

    Ok(row)
}

/// 查询玩家 QQ 绑定信息（openid 摘要 → 用于申请页/审核页展示）。
pub async fn get_binding(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<PlayerQQBinding>> {
    let row: Option<PlayerQQBinding> = sqlx::query_as(
        r#"SELECT steamid64, qq_openid, source, bound_at, unbound_at
           FROM player_qq_bindings
           WHERE steamid64 = $1 AND unbound_at IS NULL
           LIMIT 1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await?;

    Ok(row)
}

/// LumiBot 调用：使用绑定码 + 群消息发送者 openid 完成绑定。
///
/// 安全设计：openid 由 LumiBot 从消息 Author 中提取，用户无法伪造；
/// 绑定码一次性使用（用完标记 bound），且 10 分钟过期。
pub async fn bind_with_code(
    db: &Database,
    code: Uuid,
    qq_openid: &str,
) -> anyhow::Result<BindResult> {
    let qq_openid = qq_openid.trim();
    anyhow::ensure!(!qq_openid.is_empty(), "QQ openid 不能为空");

    let mut tx = db.pool.begin().await?;

    let row: Option<(String, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT steamid64, status, expires_at FROM qq_bind_codes WHERE id = $1 FOR UPDATE"#,
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((steamid64, status, expires_at)) = row else {
        tx.rollback().await?;
        return Ok(BindResult::CodeInvalid);
    };

    if status != CODE_PENDING {
        tx.rollback().await?;
        return Ok(BindResult::CodeUsed);
    }
    if expires_at < Some(Utc::now()) {
        sqlx::query("UPDATE qq_bind_codes SET status = 'expired' WHERE id = $1")
            .bind(code)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(BindResult::CodeExpired);
    }

    // 标记码已使用
    sqlx::query(
        r#"UPDATE qq_bind_codes SET status = 'bound', bound_openid = $2, bound_at = now()
           WHERE id = $1"#,
    )
    .bind(code)
    .bind(qq_openid)
    .execute(&mut *tx)
    .await?;

    // 写入持久绑定（upsert：重复绑定更新 openid 与绑定期）
    sqlx::query(
        r#"INSERT INTO player_qq_bindings (id, steamid64, qq_openid, source, bound_at)
           VALUES ($1, $2, $3, 'group_code', now())
           ON CONFLICT (steamid64) DO UPDATE
           SET qq_openid = EXCLUDED.qq_openid,
               source = EXCLUDED.source,
               bound_at = now(),
               unbound_at = NULL"#,
    )
    .bind(Uuid::new_v4())
    .bind(&steamid64)
    .bind(qq_openid)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        %code,
        steamid64 = %steamid64,
        "QQ 群绑定成功：steamid64 -> qq_openid"
    );
    Ok(BindResult::Bound { steamid64 })
}

/// 解绑：管理员手动解除绑定（预留）
#[allow(dead_code)]
pub async fn unbind(db: &Database, steamid64: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE player_qq_bindings SET unbound_at = now()
           WHERE steamid64 = $1 AND unbound_at IS NULL"#,
    )
    .bind(steamid64)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// 绑定结果
pub enum BindResult {
    /// 绑定码不存在
    CodeInvalid,
    /// 绑定码已被使用
    CodeUsed,
    /// 绑定码已过期
    CodeExpired,
    /// 绑定成功
    Bound { steamid64: String },
}

/// 玩家 QQ 绑定信息
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct PlayerQQBinding {
    pub steamid64: String,
    pub qq_openid: String,
    pub source: String,
    pub bound_at: DateTime<Utc>,
    pub unbound_at: Option<DateTime<Utc>>,
}

/// 格式化绑定联系方式：qq:<openid>
pub fn format_qq_contact(qq_openid: &str) -> String {
    format!("qq:{qq_openid}")
}

/// 从联系字段解析 QQ openid（用于展示）：qq:xxx -> xxx
#[allow(dead_code)]
pub fn parse_qq_contact(contact: Option<&str>) -> Option<&str> {
    contact
        .and_then(|c| c.strip_prefix("qq:"))
        .filter(|s| !s.is_empty())
}
