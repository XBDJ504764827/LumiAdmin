use crate::{
    auth::session::{build_session, role_label, to_view},
    db::Database,
    models::{preferred_operator_name, SessionResponse, SessionView, User},
    password::verify_password,
    services::observability_service,
};
use uuid::Uuid;

pub async fn login(
    db: &Database,
    username: &str,
    password: &str,
    ttl_hours: i64,
) -> anyhow::Result<SessionResponse> {
    // 账号维度爆破防护：与 IP 限流互补，攻击者换 IP 也无法慢速爆破同一账号。
    // 15 分钟窗口内失败 5 次即锁定 15 分钟；登录成功后清零。
    const FAILURE_WINDOW_MINS: i32 = 15;
    const MAX_FAILURES: i64 = 5;

    let user = sqlx::query_as::<_, User>(
        r#"SELECT id, username, display_name, password_hash, role, steam_id, remark, openid, whitelist_notification_enabled, enabled, created_at FROM users WHERE username = $1"#,
    )
    .bind(username)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("invalid credentials"))?;

    if !user.enabled {
        anyhow::bail!("账号已被禁用");
    }

    if !verify_password(password, &user.password_hash) {
        let failures: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM login_failures
               WHERE username = $1 AND failed_at > now() - make_interval(mins => $2)"#,
        )
        .bind(username)
        .bind(FAILURE_WINDOW_MINS)
        .fetch_one(&db.pool)
        .await?;
        sqlx::query("INSERT INTO login_failures (username, failed_at) VALUES ($1, now())")
            .bind(username)
            .execute(&db.pool)
            .await?;
        if failures.0 + 1 >= MAX_FAILURES {
            tracing::warn!(username, "登录失败次数达到阈值，账号临时锁定");
        }
        anyhow::ensure!(
            failures.0 < MAX_FAILURES,
            "登录失败次数过多，账号已临时锁定，请 15 分钟后再试"
        );
        anyhow::bail!("invalid credentials");
    }

    // 登录成功，清除该账号的失败记录
    sqlx::query("DELETE FROM login_failures WHERE username = $1")
        .bind(username)
        .execute(&db.pool)
        .await?;

    let session = build_session(&user, ttl_hours);
    sqlx::query(
        r#"INSERT INTO sessions (token, user_id, role, display_name, role_label, expires_at, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(session.token)
    .bind(session.user_id)
    .bind(&session.role)
    .bind(&session.display_name)
    .bind(&session.role_label)
    .bind(session.expires_at)
    .bind(session.created_at)
    .execute(&db.pool)
    .await?;

    Ok(SessionResponse {
        session: to_view(session),
    })
}

pub async fn current_session(db: &Database, token: Uuid) -> anyhow::Result<SessionView> {
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, Option<String>)>(
        r#"SELECT s.token, s.user_id, u.role, u.username, u.remark
           FROM sessions s
           JOIN users u ON u.id = s.user_id
           WHERE s.token = $1 AND s.expires_at > NOW()"#,
    )
    .bind(token)
    .fetch_one(&db.pool)
    .await?;

    Ok(SessionView {
        token: row.0,
        user_id: row.1,
        role: row.2.clone(),
        display_name: preferred_operator_name(&row.3, row.4.as_deref()),
        role_label: role_label(&row.2).to_string(),
    })
}

pub async fn logout(db: &Database, token: Uuid) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = $1")
        .bind(token)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// 登出指定用户的所有 session（可选保留当前 token）
pub async fn logout_all_for_user(
    db: &Database,
    user_id: Uuid,
    except_token: Option<Uuid>,
) -> anyhow::Result<u64> {
    let result = if let Some(token) = except_token {
        sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND token != $2")
            .bind(user_id)
            .bind(token)
            .execute(&db.pool)
            .await?
    } else {
        sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&db.pool)
            .await?
    };
    Ok(result.rows_affected())
}

/// 清理所有过期 session
pub async fn cleanup_expired_sessions(db: &Database) -> anyhow::Result<u64> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < NOW()")
        .execute(&db.pool)
        .await?;
    // 顺带清理 1 天前的登录失败记录（锁定窗口仅 15 分钟，保留 1 天供审计排查）
    sqlx::query("DELETE FROM login_failures WHERE failed_at < NOW() - interval '1 day'")
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected())
}

/// 启动过期 session 定时清理循环
pub fn start_session_cleanup_loop(db: Database, interval_secs: u64) {
    observability_service::register_task(
        "session_cleanup",
        "过期会话清理",
        "清理",
        Some(interval_secs),
        true,
    );
    super::task_runtime::spawn_persistent("session_cleanup", move || {
        let db = db.clone();
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                match observability_service::observe_task(
                    "session_cleanup",
                    cleanup_expired_sessions(&db),
                    |count| format!("清理 {} 个过期会话", count),
                )
                .await
                {
                    Ok(0) => {}
                    Ok(count) => tracing::info!(count, "cleaned up expired sessions"),
                    Err(e) => tracing::warn!(%e, "failed to cleanup expired sessions"),
                }
            }
        }
    });
}
