use crate::{
    auth::session::{build_session, role_label, to_view},
    db::Database,
    models::{SessionResponse, SessionView, User, preferred_operator_name},
    password::verify_password,
};
use uuid::Uuid;

pub async fn login(
    db: &Database,
    username: &str,
    password: &str,
    ttl_hours: i64,
) -> anyhow::Result<SessionResponse> {
    let user = sqlx::query_as::<_, User>(
        r#"SELECT id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at FROM users WHERE username = $1"#,
    )
    .bind(username)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("invalid credentials"))?;

    if !user.enabled {
        anyhow::bail!("账号已被禁用");
    }

    if !verify_password(password, &user.password_hash) {
        anyhow::bail!("invalid credentials");
    }

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
    Ok(result.rows_affected())
}

/// 启动过期 session 定时清理循环
pub fn start_session_cleanup_loop(db: Database, interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            match cleanup_expired_sessions(&db).await {
                Ok(0) => {}
                Ok(count) => tracing::info!(count, "cleaned up expired sessions"),
                Err(e) => tracing::warn!(%e, "failed to cleanup expired sessions"),
            }
        }
    });
}
