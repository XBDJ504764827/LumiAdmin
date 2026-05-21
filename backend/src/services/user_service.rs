use crate::{db::Database, models::{Operator, User}, password::hash_password, routes::ListQuery};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct UserListItem {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub steam_id: Option<String>,
    pub remark: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateUserInput {
    pub username: String,
    pub password: String,
    pub role: String,
    pub steam_id: Option<String>,
    pub remark: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateUserInput {
    pub username: String,
    pub role: Option<String>,
    pub steam_id: Option<String>,
    pub remark: Option<String>,
}

pub async fn find_user(db: &Database, id: Uuid) -> anyhow::Result<User> {
    sqlx::query_as::<_, User>(
        r#"SELECT id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at FROM users WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

pub async fn list_users(db: &Database, actor: &Operator, query: &ListQuery) -> anyhow::Result<crate::routes::PaginatedResponse<UserListItem>> {
    if actor.role == "normal" {
        let user = find_user(db, actor.id).await?;
        return Ok(crate::routes::PaginatedResponse {
            items: vec![UserListItem {
                id: user.id,
                username: user.username,
                display_name: user.display_name,
                role: user.role,
                steam_id: user.steam_id,
                remark: user.remark,
                enabled: user.enabled,
                created_at: user.created_at.to_rfc3339(),
            }],
            total: 1,
            page: 1,
            page_size: 20,
        });
    }

    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!("(display_name ILIKE ${param_idx} OR username ILIKE ${param_idx})"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM users {where_clause}");
    let data_sql = format!(
        r#"SELECT id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at
           FROM users {where_clause} ORDER BY created_at DESC LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, User>(&data_sql);

    if let Some(ref pattern) = search_pattern {
        count_query = count_query.bind(pattern);
        data_query = data_query.bind(pattern);
    }
    data_query = data_query.bind(query.page_size()).bind(query.offset());

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query.fetch_all(&db.pool).await?.into_iter()
        .map(|user| UserListItem {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role,
            steam_id: user.steam_id,
            remark: user.remark,
            enabled: user.enabled,
            created_at: user.created_at.to_rfc3339(),
        })
        .collect();

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn create_user(db: &Database, input: CreateUserInput) -> anyhow::Result<UserListItem> {
    let username = input.username.trim();
    let password = input.password.trim();
    let role = input.role.trim();
    let steam_id = normalize_optional_text(input.steam_id.as_deref());

    anyhow::ensure!(!username.is_empty(), "用户名不能为空");
    anyhow::ensure!(!password.is_empty(), "密码不能为空");
    anyhow::ensure!(matches!(role, "admin" | "normal"), "权限等级不合法");

    let password_hash = hash_password(password)?;
    let id = Uuid::new_v4();
    let row = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, username, display_name, password_hash, role, steam_id, remark)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at
        "#,
    )
    .bind(id)
    .bind(username)
    .bind(username)
    .bind(&password_hash)
    .bind(role)
    .bind(steam_id)
    .bind(normalize_optional_text(input.remark.as_deref()))
    .fetch_one(&db.pool)
    .await?;

    Ok(UserListItem {
        id: row.id,
        username: row.username,
        display_name: row.display_name,
        role: row.role,
        steam_id: row.steam_id,
        remark: row.remark,
        enabled: row.enabled,
        created_at: row.created_at.to_rfc3339(),
    })
}

pub async fn update_user(db: &Database, id: Uuid, input: UpdateUserInput, keep_role: bool) -> anyhow::Result<UserListItem> {
    let current = find_user(db, id).await?;
    let username = input.username.trim();
    let steam_id = normalize_optional_text(input.steam_id.as_deref());
    anyhow::ensure!(!username.is_empty(), "用户名不能为空");

    let role = if keep_role {
        current.role.clone()
    } else {
        input.role.as_deref().unwrap_or(current.role.as_str()).to_string()
    };

    anyhow::ensure!(matches!(role.as_str(), "developer" | "admin" | "normal"), "权限等级不合法");

    let row = sqlx::query_as::<_, User>(
        r#"
        UPDATE users
        SET username = $2,
            display_name = $2,
            role = $3,
            steam_id = $4,
            remark = $5
        WHERE id = $1
        RETURNING id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at
        "#,
    )
    .bind(id)
    .bind(username)
    .bind(role)
    .bind(steam_id)
    .bind(normalize_optional_text(input.remark.as_deref()))
    .fetch_one(&db.pool)
    .await?;

    Ok(UserListItem {
        id: row.id,
        username: row.username,
        display_name: row.display_name,
        role: row.role,
        steam_id: row.steam_id,
        remark: row.remark,
        enabled: row.enabled,
        created_at: row.created_at.to_rfc3339(),
    })
}

pub async fn update_password(db: &Database, id: Uuid, password: &str) -> anyhow::Result<()> {
    let password = password.trim();
    anyhow::ensure!(!password.is_empty(), "密码不能为空");

    let password_hash = hash_password(password)?;
    sqlx::query(r#"UPDATE users SET password_hash = $2 WHERE id = $1"#)
        .bind(id)
        .bind(&password_hash)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn delete_user(db: &Database, id: Uuid) -> anyhow::Result<()> {
    sqlx::query(r#"DELETE FROM users WHERE id = $1"#)
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(ToString::to_string)
}

pub async fn toggle_enabled(db: &Database, id: Uuid) -> anyhow::Result<UserListItem> {
    let row = sqlx::query_as::<_, User>(
        r#"
        UPDATE users
        SET enabled = NOT enabled
        WHERE id = $1
        RETURNING id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at
        "#,
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await?;

    Ok(UserListItem {
        id: row.id,
        username: row.username,
        display_name: row.display_name,
        role: row.role,
        steam_id: row.steam_id,
        remark: row.remark,
        enabled: row.enabled,
        created_at: row.created_at.to_rfc3339(),
    })
}
