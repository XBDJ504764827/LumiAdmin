use crate::db::Database;
use std::collections::HashMap;

#[derive(Debug, Clone, sqlx::FromRow)]
struct MapTierMysqlRow {
    map_name: String,
    tier: String,
}

/// 从 MySQL 拉取 map_tiers 全量数据并 upsert 到本地 PostgreSQL
pub async fn sync_map_tiers(pg: &Database, mysql_url: &str) -> anyhow::Result<usize> {
    let mysql_pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(mysql_url)
        .await
        .map_err(|e| anyhow::anyhow!("MySQL 连接失败: {}", e))?;

    let rows: Vec<MapTierMysqlRow> = sqlx::query_as(
        "SELECT map_name, tier FROM map_tiers",
    )
    .fetch_all(&mysql_pool)
    .await
    .map_err(|e| anyhow::anyhow!("MySQL 查询失败: {}", e))?;

    mysql_pool.close().await;

    if rows.is_empty() {
        return Ok(0);
    }

    let mut tx = pg.pool.begin().await?;

    // 批量 upsert：每批 500 条
    for chunk in rows.chunks(500) {
        let map_names: Vec<&str> = chunk.iter().map(|r| r.map_name.as_str()).collect();
        let tiers: Vec<i32> = chunk.iter().map(|r| r.tier.trim().parse().unwrap_or(0)).collect();

        sqlx::query(
            r#"INSERT INTO map_tiers (map_name, tier)
               SELECT u.map_name, u.tier
               FROM UNNEST($1::TEXT[], $2::INTEGER[]) AS u(map_name, tier)
               ON CONFLICT (map_name) DO UPDATE SET tier = EXCLUDED.tier"#,
        )
        .bind(&map_names)
        .bind(&tiers)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(rows.len())
}

/// 查询多个地图的等级，返回 map_name -> tier 映射
pub async fn get_map_tiers(db: &Database, map_names: &[String]) -> anyhow::Result<HashMap<String, i32>> {
    if map_names.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT map_name, tier FROM map_tiers WHERE map_name = ANY($1)",
    )
    .bind(map_names)
    .fetch_all(&db.pool)
    .await?;

    Ok(rows.into_iter().collect())
}

/// 启动定时同步循环
pub fn start_sync_loop(db: Database, mysql_url: String, interval_secs: u64) {
    tokio::spawn(async move {
        match sync_map_tiers(&db, &mysql_url).await {
            Ok(count) => tracing::info!(count, "map_tiers 初始同步完成"),
            Err(e) => tracing::warn!(%e, "map_tiers 初始同步失败"),
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            match sync_map_tiers(&db, &mysql_url).await {
                Ok(count) => tracing::info!(count, "map_tiers 定时同步完成"),
                Err(e) => tracing::warn!(%e, "map_tiers 定时同步失败"),
            }
        }
    });
}
