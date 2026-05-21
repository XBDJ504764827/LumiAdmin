use crate::{auth::session::{build_session, to_view}, db::Database, models::{SessionResponse, SessionView, User}, password::verify_password};
use uuid::Uuid;

pub async fn login(db: &Database, username: &str, password: &str, ttl_hours: i64) -> anyhow::Result<SessionResponse> {
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

    Ok(SessionResponse { session: to_view(session) })
}

pub async fn current_session(db: &Database, token: Uuid) -> anyhow::Result<SessionView> {
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, String)>(
        r#"SELECT token, user_id, role, display_name, role_label
           FROM sessions WHERE token = $1 AND expires_at > NOW()"#,
    )
    .bind(token)
    .fetch_one(&db.pool)
    .await?;

    Ok(SessionView {
        token: row.0,
        user_id: row.1,
        display_name: row.3,
        role: row.2,
        role_label: row.4,
    })
}

pub async fn logout(db: &Database, token: Uuid) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = $1").bind(token).execute(&db.pool).await?;
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
