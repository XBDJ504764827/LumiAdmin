use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 玩家进服权限配置记录
#[derive(Debug, Clone, Serialize)]
pub struct PlayerAccessRule {
    pub id: Uuid,
    pub steamid64: String,
    pub nickname: String,
    /// 允许进入的社区 ID 列表（空表示无特殊允许）
    pub allowed_communities: Vec<Uuid>,
    /// 禁止进入的社区 ID 列表（空表示无特殊禁止）
    pub blocked_communities: Vec<Uuid>,
    /// 允许进入的服务器 ID 列表（优先级高于社区设置）
    pub allowed_servers: Vec<Uuid>,
    /// 禁止进入的服务器 ID 列表（优先级高于社区设置）
    pub blocked_servers: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePlayerAccessRuleInput {
    pub steamid64: String,
    pub nickname: String,
    pub allowed_communities: Option<Vec<Uuid>>,
    pub blocked_communities: Option<Vec<Uuid>>,
    pub allowed_servers: Option<Vec<Uuid>>,
    pub blocked_servers: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePlayerAccessRuleInput {
    pub nickname: Option<String>,
    pub allowed_communities: Option<Vec<Uuid>>,
    pub blocked_communities: Option<Vec<Uuid>>,
    pub allowed_servers: Option<Vec<Uuid>>,
    pub blocked_servers: Option<Vec<Uuid>>,
}

#[derive(sqlx::FromRow)]
struct PlayerAccessRuleRow {
    id: Uuid,
    steamid64: String,
    nickname: String,
    allowed_communities: Vec<Uuid>,
    blocked_communities: Vec<Uuid>,
    allowed_servers: Vec<Uuid>,
    blocked_servers: Vec<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// 获取所有特殊权限配置
pub async fn list_rules(db: &Database) -> anyhow::Result<Vec<PlayerAccessRule>> {
    let rows = sqlx::query_as::<_, PlayerAccessRuleRow>(
        r#"SELECT id, steamid64, nickname, allowed_communities, blocked_communities,
                  allowed_servers, blocked_servers, created_at, updated_at
           FROM player_access_rules
           ORDER BY updated_at DESC"#,
    )
    .fetch_all(&db.pool)
    .await?;

    Ok(rows.into_iter().map(map_row_to_rule).collect())
}

/// 根据 SteamID64 查找规则
pub async fn find_rule_by_steamid64(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<PlayerAccessRule>> {
    let row = sqlx::query_as::<_, PlayerAccessRuleRow>(
        r#"SELECT id, steamid64, nickname, allowed_communities, blocked_communities,
                  allowed_servers, blocked_servers, created_at, updated_at
           FROM player_access_rules
           WHERE steamid64 = $1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await?;

    Ok(row.map(map_row_to_rule))
}

/// 根据 ID 查找规则
pub async fn find_rule_by_id(db: &Database, id: Uuid) -> anyhow::Result<Option<PlayerAccessRule>> {
    let row = sqlx::query_as::<_, PlayerAccessRuleRow>(
        r#"SELECT id, steamid64, nickname, allowed_communities, blocked_communities,
                  allowed_servers, blocked_servers, created_at, updated_at
           FROM player_access_rules
           WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await?;

    Ok(row.map(map_row_to_rule))
}

/// 创建新规则
pub async fn create_rule(
    db: &Database,
    input: CreatePlayerAccessRuleInput,
) -> anyhow::Result<PlayerAccessRule> {
    let steamid64 = input.steamid64.trim();
    anyhow::ensure!(!steamid64.is_empty(), "SteamID64 不能为空");
    anyhow::ensure!(!input.nickname.trim().is_empty(), "玩家昵称不能为空");

    // 检查是否已存在
    if find_rule_by_steamid64(db, steamid64).await?.is_some() {
        anyhow::bail!("该玩家已有进服权限配置");
    }

    let row = sqlx::query_as::<_, PlayerAccessRuleRow>(
        r#"INSERT INTO player_access_rules (
            id, steamid64, nickname, allowed_communities, blocked_communities,
            allowed_servers, blocked_servers, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
        RETURNING id, steamid64, nickname, allowed_communities, blocked_communities,
                  allowed_servers, blocked_servers, created_at, updated_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(steamid64)
    .bind(input.nickname.trim())
    .bind(input.allowed_communities.unwrap_or_default())
    .bind(input.blocked_communities.unwrap_or_default())
    .bind(input.allowed_servers.unwrap_or_default())
    .bind(input.blocked_servers.unwrap_or_default())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_row_to_rule(row))
}

/// 更新规则
pub async fn update_rule(
    db: &Database,
    id: Uuid,
    input: UpdatePlayerAccessRuleInput,
) -> anyhow::Result<PlayerAccessRule> {
    let _existing = find_rule_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("规则不存在"))?;

    let row = sqlx::query_as::<_, PlayerAccessRuleRow>(
        r#"UPDATE player_access_rules
           SET nickname = COALESCE($2, nickname),
               allowed_communities = COALESCE($3, allowed_communities),
               blocked_communities = COALESCE($4, blocked_communities),
               allowed_servers = COALESCE($5, allowed_servers),
               blocked_servers = COALESCE($6, blocked_servers),
               updated_at = now()
           WHERE id = $1
           RETURNING id, steamid64, nickname, allowed_communities, blocked_communities,
                     allowed_servers, blocked_servers, created_at, updated_at"#,
    )
    .bind(id)
    .bind(input.nickname.map(|n| n.trim().to_string()))
    .bind(input.allowed_communities)
    .bind(input.blocked_communities)
    .bind(input.allowed_servers)
    .bind(input.blocked_servers)
    .fetch_one(&db.pool)
    .await?;

    Ok(map_row_to_rule(row))
}

/// 删除规则（重置为默认）
pub async fn delete_rule(db: &Database, id: Uuid) -> anyhow::Result<()> {
    sqlx::query(r#"DELETE FROM player_access_rules WHERE id = $1"#)
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// 检查玩家是否可以进入指定服务器
/// 返回 (是否允许, 拒绝原因, 是否被自定义规则明确放行)
pub async fn check_player_access(
    db: &Database,
    steamid64: &str,
    server_id: Uuid,
    community_id: Uuid,
) -> anyhow::Result<(bool, Option<String>, bool)> {
    // 查找玩家的特殊规则
    let rule = find_rule_by_steamid64(db, steamid64).await?;

    let Some(rule) = rule else {
        // 没有特殊规则，默认允许（但不是由自定义规则放行）
        return Ok((true, None, false));
    };

    // 检查服务器级别（优先级最高）
    if rule.allowed_servers.contains(&server_id) {
        return Ok((true, None, true));
    }
    if rule.blocked_servers.contains(&server_id) {
        return Ok((false, Some("您被禁止进入该服务器".to_string()), false));
    }

    // 检查社区级别
    if rule.allowed_communities.contains(&community_id) {
        return Ok((true, None, true));
    }
    if rule.blocked_communities.contains(&community_id) {
        return Ok((false, Some("您被禁止进入该社区的所有服务器".to_string()), false));
    }

    // 有规则但没有匹配到 allow/block，默认允许（视为规则放行）
    Ok((true, None, true))
}

fn map_row_to_rule(row: PlayerAccessRuleRow) -> PlayerAccessRule {
    PlayerAccessRule {
        id: row.id,
        steamid64: row.steamid64,
        nickname: row.nickname,
        allowed_communities: row.allowed_communities,
        blocked_communities: row.blocked_communities,
        allowed_servers: row.allowed_servers,
        blocked_servers: row.blocked_servers,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database};
    use sqlx::postgres::PgPoolOptions;

    fn schema_url(base_url: &str, schema: &str) -> String {
        let separator = if base_url.contains('?') { '&' } else { '?' };
        format!("{base_url}{separator}options=-csearch_path%3D{schema}")
    }

    async fn create_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(base_url)
            .await
            .unwrap();
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    async fn drop_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(base_url)
            .await
            .unwrap();
        sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn create_and_find_rule() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let rule = create_rule(
                &db,
                CreatePlayerAccessRuleInput {
                    steamid64: "76561198000000001".to_string(),
                    nickname: "测试玩家".to_string(),
                    allowed_communities: None,
                    blocked_communities: None,
                    allowed_servers: None,
                    blocked_servers: None,
                },
            )
            .await?;

            assert_eq!(rule.steamid64, "76561198000000001");
            assert_eq!(rule.nickname, "测试玩家");

            let found = find_rule_by_steamid64(&db, "76561198000000001").await?;
            assert!(found.is_some());

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn check_access_with_blocked_server() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);

        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let server_id = Uuid::new_v4();
            let community_id = Uuid::new_v4();

            create_rule(
                &db,
                CreatePlayerAccessRuleInput {
                    steamid64: "76561198000000002".to_string(),
                    nickname: "受限玩家".to_string(),
                    allowed_communities: None,
                    blocked_communities: None,
                    allowed_servers: None,
                    blocked_servers: Some(vec![server_id]),
                },
            )
            .await?;

            let (allowed, reason, _has_rule) =
                check_player_access(&db, "76561198000000002", server_id, community_id).await?;
            assert!(!allowed);
            assert!(reason.is_some());

            Ok::<(), anyhow::Error>(())
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }
}
