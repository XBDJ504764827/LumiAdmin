//! 白名单两步验证：Steam ↔ QQ 群绑定服务
//!
//! 流程：
//! 1. 玩家在网站完成 Steam 身份验证后，请求生成一次性验证码（默认 5 分钟有效）。
//! 2. 玩家加入 QQ 群并在群内 @机器人 发送该验证码。
//! 3. LumiBot 调用 `/api/integration/qq/bind/verify` 校验并写入 `steam_qq_bindings`。
//! 4. 玩家回到网站，系统确认绑定后即可填写理由并提交白名单申请。
//!
//! 设计要点：
//! - 验证码单个 Steam 仅保留一条活跃码，重新生成即作废上一条；
//! - 绑定关系以 `steamid64` 唯一，1 个 QQ(openid) 最多绑定 N 个 Steam（默认 5）；
//! - 绑定永久有效，`revoked`/`expired` 重新申请时复用绑定，无需重新加群；
//! - 换绑需要管理员先删除绑定（`delete_binding`）。

use crate::db::Database;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// 默认验证码有效期（秒）：5 分钟
pub const DEFAULT_CODE_TTL_SECONDS: i32 = 300;

/// 默认单 QQ 最多绑定 Steam 数
pub const DEFAULT_MAX_BINDINGS: i32 = 5;

/// 验证码字符集：去掉容易混淆的 0/O/1/I/L
const CODE_CHARSET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;

/// QQ 群配置（单行表）。
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct QqConfig {
    pub enabled: bool,
    pub group_number: String,
    pub group_link: Option<String>,
    pub group_openids: Vec<String>,
    pub max_bindings: i32,
    pub code_ttl_seconds: i32,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl QqConfig {
    /// 配置默认值（表为空时兜底）
    fn default_row() -> Self {
        Self {
            enabled: true,
            group_number: "275164688".to_string(),
            group_link: Some("https://qm.qq.com/q/vRqmiKS6oE".to_string()),
            group_openids: Vec::new(),
            max_bindings: DEFAULT_MAX_BINDINGS,
            code_ttl_seconds: DEFAULT_CODE_TTL_SECONDS,
            updated_by: None,
            updated_at: Utc::now(),
        }
    }

    /// 是否允许该群 openid 参与绑定（白名单为空表示不限制）
    pub fn allows_group(&self, group_openid: &str) -> bool {
        self.group_openids.is_empty() || self.group_openids.iter().any(|g| g == group_openid)
    }
}

/// 读取 QQ 群配置（表为空时返回默认值）。
pub async fn load_config(db: &Database) -> anyhow::Result<QqConfig> {
    let row: Option<QqConfig> = sqlx::query_as(
        r#"SELECT enabled, group_number, group_link, group_openids,
                  max_bindings, code_ttl_seconds, updated_by, updated_at
           FROM whitelist_qq_config
           WHERE id = true"#,
    )
    .fetch_optional(&db.pool)
    .await
    .context("读取白名单 QQ 群配置失败")?;
    Ok(row.unwrap_or_else(QqConfig::default_row))
}

/// 更新 QQ 群配置（管理后台）。
#[allow(clippy::too_many_arguments)]
pub async fn update_config(
    db: &Database,
    enabled: bool,
    group_number: &str,
    group_link: Option<&str>,
    group_openids: Vec<String>,
    max_bindings: i32,
    code_ttl_seconds: i32,
    updated_by: &str,
) -> anyhow::Result<QqConfig> {
    let max_bindings = max_bindings.clamp(1, 50);
    // 验证码有效期限制在 1 分钟 ~ 1 小时，避免过短或过长
    let code_ttl_seconds = code_ttl_seconds.clamp(60, 3600);
    let group_openids: Vec<String> = group_openids
        .into_iter()
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty())
        .collect();
    sqlx::query(
        r#"INSERT INTO whitelist_qq_config
             (id, enabled, group_number, group_link, group_openids, max_bindings, code_ttl_seconds, updated_by, updated_at)
           VALUES (true, $1, $2, $3, $4, $5, $6, $7, now())
           ON CONFLICT (id) DO UPDATE
           SET enabled = EXCLUDED.enabled,
               group_number = EXCLUDED.group_number,
               group_link = EXCLUDED.group_link,
               group_openids = EXCLUDED.group_openids,
               max_bindings = EXCLUDED.max_bindings,
               code_ttl_seconds = EXCLUDED.code_ttl_seconds,
               updated_by = EXCLUDED.updated_by,
               updated_at = now()"#,
    )
    .bind(enabled)
    .bind(group_number.trim())
    .bind(group_link.map(str::trim).filter(|s| !s.is_empty()))
    .bind(&group_openids)
    .bind(max_bindings)
    .bind(code_ttl_seconds)
    .bind(updated_by.trim())
    .execute(&db.pool)
    .await
    .context("更新白名单 QQ 群配置失败")?;
    load_config(db).await
}

/// Steam ↔ QQ 绑定记录
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SteamQqBinding {
    pub id: Uuid,
    pub steamid64: String,
    pub qq_openid: String,
    pub qq_group_id: String,
    pub qq_username: Option<String>,
    pub verified_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 生成的验证码结果
#[derive(Debug, Clone, Serialize)]
pub struct IssuedCode {
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub group_number: String,
    pub group_link: Option<String>,
    pub ttl_seconds: i32,
}

/// 生成验证码（单个 Steam 仅保留一条活跃码，重新生成作废旧码）。
pub async fn issue_code(
    db: &Database,
    steamid64: &str,
    created_ip: Option<&str>,
) -> anyhow::Result<IssuedCode> {
    let config = load_config(db).await?;

    // 已有绑定则不允许再生成：玩家应直接进入下一步
    if let Some(binding) = find_binding_by_steamid64(db, steamid64).await? {
        anyhow::bail!(
            "该 Steam 账号已绑定 QQ（{}），无需重复验证",
            mask_openid(&binding.qq_openid)
        );
    }

    // 作废该 Steam 之前未消费的验证码
    sqlx::query(
        r#"UPDATE whitelist_qq_verify_codes
           SET consumed_at = now()
           WHERE steamid64 = $1 AND consumed_at IS NULL"#,
    )
    .bind(steamid64)
    .execute(&db.pool)
    .await
    .context("作废旧验证码失败")?;

    let expires_at = Utc::now() + chrono::Duration::seconds(config.code_ttl_seconds as i64);

    // 生成唯一验证码：极小概率冲突时重试几次
    let mut last_err: Option<anyhow::Error> = None;
    for _ in 0..5 {
        let code = format!("WL-{}", random_code());
        let result = sqlx::query(
            r#"INSERT INTO whitelist_qq_verify_codes
                 (id, steamid64, code, expires_at, created_ip)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(steamid64)
        .bind(&code)
        .bind(expires_at)
        .bind(created_ip)
        .execute(&db.pool)
        .await;
        match result {
            Ok(_) => {
                return Ok(IssuedCode {
                    code,
                    expires_at,
                    group_number: config.group_number,
                    group_link: config.group_link,
                    ttl_seconds: config.code_ttl_seconds,
                });
            }
            Err(e) => {
                if let Some(sqlx_err) = e.as_database_error() {
                    if sqlx_err.is_unique_violation() {
                        last_err = Some(anyhow::anyhow!("验证码冲突，请重试"));
                        continue;
                    }
                }
                return Err(e).context("写入验证码失败");
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("生成验证码失败，请重试")))
}

/// 绑定校验结果：供 LumiBot 接口区分回复文案。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum BindOutcome {
    /// 绑定成功
    Bound {
        steamid64: String,
        qq_openid: String,
        qq_group_id: String,
        already: bool,
    },
    /// 验证码不存在或已过期
    InvalidCode,
    /// 验证码已被使用
    Consumed,
    /// 该 QQ 绑定数量已达上限
    LimitReached { max: i32 },
    /// 该 QQ 群不在允许列表
    GroupDenied,
    /// 功能未开启
    Disabled,
}

/// 校验验证码并写入绑定（供 LumiBot 调用）。
///
/// 并发安全：通过 `UPDATE ... WHERE consumed_at IS NULL` 原子消费验证码，
/// 两位玩家同时提交同一验证码时只有一个能成功。
pub async fn verify_and_bind(
    db: &Database,
    code: &str,
    qq_openid: &str,
    qq_group_id: &str,
    qq_username: Option<&str>,
) -> anyhow::Result<BindOutcome> {
    let config = load_config(db).await?;
    if !config.enabled {
        return Ok(BindOutcome::Disabled);
    }
    if !config.allows_group(qq_group_id) {
        return Ok(BindOutcome::GroupDenied);
    }

    let code = code.trim();
    anyhow::ensure!(!code.is_empty(), "验证码不能为空");
    anyhow::ensure!(!qq_openid.trim().is_empty(), "QQ 用户标识不能为空");
    anyhow::ensure!(!qq_group_id.trim().is_empty(), "QQ 群标识不能为空");

    // 查询验证码（含已过期/已消费，用于区分提示文案）
    /// (id, steamid64, expires_at, consumed_at)
    type CodeRow = (Uuid, String, DateTime<Utc>, Option<DateTime<Utc>>);
    let row: Option<CodeRow> = sqlx::query_as(
        r#"SELECT id, steamid64, expires_at, consumed_at
           FROM whitelist_qq_verify_codes
           WHERE code = $1"#,
    )
    .bind(code)
    .fetch_optional(&db.pool)
    .await
    .context("查询验证码失败")?;

    let (code_id, steamid64, expires_at, consumed_at) = match row {
        Some(row) => row,
        None => return Ok(BindOutcome::InvalidCode),
    };
    if consumed_at.is_some() {
        return Ok(BindOutcome::Consumed);
    }
    if expires_at <= Utc::now() {
        return Ok(BindOutcome::InvalidCode);
    }

    // 已有绑定：若绑定信息一致则视为幂等成功，否则拒绝（换绑需管理员解绑）
    if let Some(existing) = find_binding_by_steamid64(db, &steamid64).await? {
        if existing.qq_openid == qq_openid {
            mark_code_consumed(db, code_id, qq_openid).await?;
            return Ok(BindOutcome::Bound {
                steamid64,
                qq_openid: qq_openid.to_string(),
                qq_group_id: existing.qq_group_id,
                already: true,
            });
        }
        anyhow::bail!("该 Steam 账号已绑定其他 QQ，如需换绑请联系管理员解绑");
    }

    let mut tx = db.pool.begin().await?;

    // 原子消费验证码
    let consumed = sqlx::query(
        r#"UPDATE whitelist_qq_verify_codes
           SET consumed_at = now(), consumed_by_qq_openid = $2
           WHERE id = $1 AND consumed_at IS NULL"#,
    )
    .bind(code_id)
    .bind(qq_openid)
    .execute(&mut *tx)
    .await
    .context("消费验证码失败")?
    .rows_affected();
    if consumed == 0 {
        tx.rollback().await.ok();
        return Ok(BindOutcome::Consumed);
    }

    // 校验该 QQ 绑定数量上限
    let bound_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM steam_qq_bindings WHERE qq_openid = $1"#)
            .bind(qq_openid)
            .fetch_one(&mut *tx)
            .await
            .context("统计 QQ 绑定数量失败")?;
    if bound_count >= config.max_bindings as i64 {
        tx.rollback().await.ok();
        return Ok(BindOutcome::LimitReached {
            max: config.max_bindings,
        });
    }

    sqlx::query(
        r#"INSERT INTO steam_qq_bindings
             (id, steamid64, qq_openid, qq_group_id, qq_username)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (steamid64) DO UPDATE
           SET qq_openid = EXCLUDED.qq_openid,
               qq_group_id = EXCLUDED.qq_group_id,
               qq_username = EXCLUDED.qq_username,
               verified_at = now(),
               updated_at = now()"#,
    )
    .bind(Uuid::new_v4())
    .bind(&steamid64)
    .bind(qq_openid)
    .bind(qq_group_id)
    .bind(qq_username.map(str::trim).filter(|s| !s.is_empty()))
    .execute(&mut *tx)
    .await
    .context("写入 Steam-QQ 绑定失败")?;

    tx.commit().await?;

    Ok(BindOutcome::Bound {
        steamid64,
        qq_openid: qq_openid.to_string(),
        qq_group_id: qq_group_id.to_string(),
        already: false,
    })
}

async fn mark_code_consumed(db: &Database, code_id: Uuid, qq_openid: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE whitelist_qq_verify_codes
           SET consumed_at = now(), consumed_by_qq_openid = $2
           WHERE id = $1 AND consumed_at IS NULL"#,
    )
    .bind(code_id)
    .bind(qq_openid)
    .execute(&db.pool)
    .await
    .context("消费验证码失败")?;
    Ok(())
}

/// 按 SteamID64 查询绑定。
pub async fn find_binding_by_steamid64(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<SteamQqBinding>> {
    sqlx::query_as::<_, SteamQqBinding>(
        r#"SELECT id, steamid64, qq_openid, qq_group_id, qq_username,
                  verified_at, created_at, updated_at
           FROM steam_qq_bindings
           WHERE steamid64 = $1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .context("查询 Steam-QQ 绑定失败")
}

/// 统计某个 QQ 已绑定的 Steam 数量。
pub async fn count_bindings_for_qq(db: &Database, qq_openid: &str) -> anyhow::Result<i64> {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM steam_qq_bindings WHERE qq_openid = $1"#)
        .bind(qq_openid)
        .fetch_one(&db.pool)
        .await
        .context("统计 QQ 绑定数量失败")
}

/// 管理员解绑（换绑前必须先解绑）。返回被删除的记录。
pub async fn delete_binding(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<SteamQqBinding>> {
    let row = sqlx::query_as::<_, SteamQqBinding>(
        r#"DELETE FROM steam_qq_bindings
           WHERE steamid64 = $1
           RETURNING id, steamid64, qq_openid, qq_group_id, qq_username,
                     verified_at, created_at, updated_at"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .context("删除 Steam-QQ 绑定失败")?;
    Ok(row)
}

/// 列出某个 QQ 绑定的全部 Steam（供管理端排查多开）。
pub async fn list_bindings_for_qq(
    db: &Database,
    qq_openid: &str,
) -> anyhow::Result<Vec<SteamQqBinding>> {
    sqlx::query_as::<_, SteamQqBinding>(
        r#"SELECT id, steamid64, qq_openid, qq_group_id, qq_username,
                  verified_at, created_at, updated_at
           FROM steam_qq_bindings
           WHERE qq_openid = $1
           ORDER BY verified_at DESC"#,
    )
    .bind(qq_openid)
    .fetch_all(&db.pool)
    .await
    .context("查询 QQ 绑定列表失败")
}

/// 生成随机验证码串（大写去混淆字符）。
fn random_code() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // 随机源：线程本地计数器 + 纳秒时间，拼接后做简单打散。
    // 验证码仅 6 位且 5 分钟过期，配合限流足以抵御爆破，无需加密级随机。
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut state = nanos as u64 ^ (Uuid::new_v4().as_u128() as u64);
    let mut out = String::with_capacity(CODE_LENGTH);
    for _ in 0..CODE_LENGTH {
        // xorshift64*
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let value = state.wrapping_mul(0x2545F4914F6CDD1D);
        let idx = (value % CODE_CHARSET.len() as u64) as usize;
        out.push(CODE_CHARSET[idx] as char);
    }
    out
}

/// 脱敏展示 openid（日志/提示用），只保留首尾少量字符。
pub fn mask_openid(openid: &str) -> String {
    let len = openid.chars().count();
    if len <= 8 {
        return "****".to_string();
    }
    let head: String = openid.chars().take(4).collect();
    let tail: String = openid.chars().skip(len - 4).collect();
    format!("{head}****{tail}")
}

/// 群内 @玩家 记录（成功/失败均落库）
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct QqMentionLog {
    pub id: Uuid,
    pub steamid64: String,
    pub qq_openid: String,
    pub qq_group_id: String,
    pub content: String,
    pub status: String,
    pub error: Option<String>,
    pub operator_id: Option<Uuid>,
    pub operator_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 写入一条群内 @玩家 记录（成功或失败）。
#[allow(clippy::too_many_arguments)]
pub async fn record_mention(
    db: &Database,
    steamid64: &str,
    qq_openid: &str,
    qq_group_id: &str,
    content: &str,
    status: &str,
    error: Option<&str>,
    operator_id: Option<Uuid>,
    operator_name: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO qq_mention_logs
             (id, steamid64, qq_openid, qq_group_id, content, status, error, operator_id, operator_name)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(Uuid::new_v4())
    .bind(steamid64)
    .bind(qq_openid)
    .bind(qq_group_id)
    .bind(content)
    .bind(status)
    .bind(error)
    .bind(operator_id)
    .bind(operator_name)
    .execute(&db.pool)
    .await
    .context("写入 QQ 群内通知记录失败")?;
    Ok(())
}

/// 最近一次成功发送时间（用于冷却判断）。
pub async fn last_mention_at(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<DateTime<Utc>>> {
    let row: Option<DateTime<Utc>> = sqlx::query_scalar(
        r#"SELECT MAX(created_at) FROM qq_mention_logs
           WHERE steamid64 = $1 AND status = 'sent'"#,
    )
    .bind(steamid64)
    .fetch_one(&db.pool)
    .await
    .context("查询最近一次群内通知时间失败")?;
    Ok(row)
}

/// 最近一条群内 @玩家 记录（供前端展示发送结果）。
pub async fn latest_mention(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<QqMentionLog>> {
    sqlx::query_as::<_, QqMentionLog>(
        r#"SELECT id, steamid64, qq_openid, qq_group_id, content, status, error,
                  operator_id, operator_name, created_at
           FROM qq_mention_logs
           WHERE steamid64 = $1
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .context("查询 QQ 群内通知记录失败")
}

/// 群内 @玩家 冷却秒数（同玩家）。
pub const MENTION_COOLDOWN_SECONDS: i64 = 60;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database};
    use uuid::Uuid;

    fn schema_url(base_url: &str, schema: &str) -> String {
        crate::test_util::schema_url(base_url, schema)
    }

    async fn create_schema(base_url: &str, schema: &str) {
        crate::test_util::create_schema(base_url, schema).await;
    }

    async fn drop_schema(base_url: &str, schema: &str) {
        crate::test_util::drop_schema(base_url, schema).await;
    }

    async fn with_test_db(test: impl AsyncFnOnce(Database) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url;
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            test(db).await
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn issue_and_bind_steam_to_qq() {
        with_test_db(async |db| {
            let issued = issue_code(&db, "76561198000000001", Some("127.0.0.1"))
                .await
                .unwrap();
            assert!(issued.code.starts_with("WL-"));
            assert_eq!(issued.ttl_seconds, DEFAULT_CODE_TTL_SECONDS);

            let outcome = verify_and_bind(
                &db,
                &issued.code,
                "qq-openid-1",
                "group-openid-1",
                Some("玩家甲"),
            )
            .await
            .unwrap();
            assert!(matches!(outcome, BindOutcome::Bound { already: false, .. }));

            let binding = find_binding_by_steamid64(&db, "76561198000000001")
                .await
                .unwrap()
                .expect("binding should exist");
            assert_eq!(binding.qq_openid, "qq-openid-1");
            assert_eq!(binding.qq_username.as_deref(), Some("玩家甲"));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn issuing_new_code_invalidates_previous() {
        with_test_db(async |db| {
            let first = issue_code(&db, "76561198000000002", None).await.unwrap();
            let second = issue_code(&db, "76561198000000002", None).await.unwrap();
            assert_ne!(first.code, second.code);

            let outcome = verify_and_bind(&db, &first.code, "qq-2", "group-2", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::Consumed));

            let outcome = verify_and_bind(&db, &second.code, "qq-2", "group-2", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::Bound { already: false, .. }));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn code_is_single_use() {
        with_test_db(async |db| {
            let issued = issue_code(&db, "76561198000000003", None).await.unwrap();
            let outcome = verify_and_bind(&db, &issued.code, "qq-3a", "group-3", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::Bound { .. }));

            let outcome = verify_and_bind(&db, &issued.code, "qq-3b", "group-3", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::Consumed));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn unknown_code_reports_invalid() {
        with_test_db(async |db| {
            let outcome = verify_and_bind(&db, "WL-NOPE00", "qq-x", "group-x", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::InvalidCode));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn qq_limit_blocks_extra_bindings() {
        with_test_db(async |db| {
            update_config(&db, true, "275164688", None, vec![], 2, 300, "tester")
                .await
                .unwrap();

            for (i, steam) in ["76561198000000011", "76561198000000012"]
                .iter()
                .enumerate()
            {
                let issued = issue_code(&db, steam, None).await.unwrap();
                let outcome = verify_and_bind(
                    &db,
                    &issued.code,
                    "qq-limit",
                    "group-limit",
                    Some(&format!("玩家{i}")),
                )
                .await
                .unwrap();
                assert!(matches!(outcome, BindOutcome::Bound { .. }));
            }

            let issued = issue_code(&db, "76561198000000013", None).await.unwrap();
            let outcome = verify_and_bind(&db, &issued.code, "qq-limit", "group-limit", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::LimitReached { max: 2 }));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn group_allowlist_rejects_unknown_group() {
        with_test_db(async |db| {
            update_config(
                &db,
                true,
                "275164688",
                None,
                vec!["allowed-group".to_string()],
                5,
                300,
                "tester",
            )
            .await
            .unwrap();

            let issued = issue_code(&db, "76561198000000021", None).await.unwrap();
            let outcome = verify_and_bind(&db, &issued.code, "qq-g", "other-group", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::GroupDenied));

            let outcome = verify_and_bind(&db, &issued.code, "qq-g", "allowed-group", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::Bound { .. }));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn delete_binding_allows_rebind() {
        with_test_db(async |db| {
            let issued = issue_code(&db, "76561198000000031", None).await.unwrap();
            verify_and_bind(&db, &issued.code, "qq-old", "group-1", None)
                .await
                .unwrap();

            let removed = delete_binding(&db, "76561198000000031")
                .await
                .unwrap()
                .expect("binding should be removed");
            assert_eq!(removed.qq_openid, "qq-old");

            let issued = issue_code(&db, "76561198000000031", None).await.unwrap();
            let outcome = verify_and_bind(&db, &issued.code, "qq-new", "group-1", None)
                .await
                .unwrap();
            assert!(matches!(outcome, BindOutcome::Bound { .. }));
            Ok(())
        })
        .await;
    }
}
