//! 游戏节点（Game Node）服务。
//!
//! 同一台游戏机（共享游戏文件 / SourceMod 目录）上的所有服务器归为同一节点。
//! 节点是授权同步的最小单位：节点内所有服务器进程共享同一份本地 SQLite 快照，
//! 只有一个 leader 进程轮询网站，避免「N 台服拉 N 次快照」以及部分服同步不一致。
//!
//! 节点按服务器 IP 自动归组；ip 变化时重新归组。

use crate::db::Database;
use uuid::Uuid;

/// 按 IP 找到或创建节点，返回节点 ID。`ip` 为空时返回 None（无法归组）。
pub async fn ensure_node_for_ip(db: &Database, ip: &str) -> anyhow::Result<Option<Uuid>> {
    let name = ip.trim();
    if name.is_empty() {
        return Ok(None);
    }

    // 先查后插 + ON CONFLICT 兜底（name 未加唯一约束，用事务内查询避免并发重复）。
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM game_nodes WHERE name = $1 ORDER BY created_at LIMIT 1")
            .bind(name)
            .fetch_optional(&db.pool)
            .await?;
    if let Some((id,)) = existing {
        return Ok(Some(id));
    }

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO game_nodes (id, name) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(id)
        .bind(name)
        .execute(&db.pool)
        .await?;
    // 并发下可能已被其它请求插入同名节点，回查一次保证返回真实记录
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM game_nodes WHERE name = $1 ORDER BY created_at LIMIT 1")
            .bind(name)
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|(id,)| id).or(Some(id)))
}

/// 解析服务器所属节点；若尚未分配则按服务器 IP 自动补建并回写。
/// 兼容历史数据 / 直接写库创建的服务器。
pub async fn resolve_or_assign_node(
    db: &Database,
    server_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    if let Some(node_id) = node_for_server(db, server_id).await? {
        return Ok(Some(node_id));
    }
    let row: Option<(String,)> = sqlx::query_as("SELECT ip FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(&db.pool)
        .await?;
    let Some((ip,)) = row else {
        return Ok(None);
    };
    let Some(node_id) = ensure_node_for_ip(db, &ip).await? else {
        return Ok(None);
    };
    sqlx::query("UPDATE servers SET node_id = $2 WHERE id = $1")
        .bind(server_id)
        .bind(node_id)
        .execute(&db.pool)
        .await?;
    Ok(Some(node_id))
}

pub async fn node_for_server_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    server_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    let row: Option<(Option<Uuid>,)> = sqlx::query_as("SELECT node_id FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(row.and_then(|(node_id,)| node_id))
}

pub async fn node_for_server(db: &Database, server_id: Uuid) -> anyhow::Result<Option<Uuid>> {
    let row: Option<(Option<Uuid>,)> = sqlx::query_as("SELECT node_id FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row.and_then(|(node_id,)| node_id))
}

/// 列出节点及其服务器数量，供后台展示/排查。
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct GameNodeItem {
    pub id: Uuid,
    pub name: String,
    pub token: String,
    pub server_count: i64,
}

#[allow(dead_code)]
pub async fn list_nodes(db: &Database) -> anyhow::Result<Vec<GameNodeItem>> {
    sqlx::query_as::<_, GameNodeItem>(
        r#"SELECT g.id, g.name, g.token,
                  COUNT(s.id)::BIGINT AS server_count
           FROM game_nodes g
           LEFT JOIN servers s ON s.node_id = g.id
           GROUP BY g.id, g.name, g.token
           ORDER BY g.name"#,
    )
    .fetch_all(&db.pool)
    .await
    .map_err(Into::into)
}
