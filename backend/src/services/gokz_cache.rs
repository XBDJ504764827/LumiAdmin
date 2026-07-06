use crate::{db::Database, services::observability_service};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// GOKZ 玩家统计数据（4 个 mode）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GokzStats {
    pub kzt: Option<GokzModeStats>,
    pub skz: Option<GokzModeStats>,
    pub vnl: Option<GokzModeStats>,
    pub ovr: Option<GokzModeStats>,
}

/// 单个 mode 的 GOKZ 统计数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GokzModeStats {
    pub rating: Option<f64>,
    pub rank: Option<i64>,
    pub points: Option<f64>,
    pub unique_map_finishes: Option<i64>,
}

/// 缓存配置
const CACHE_TTL_HOURS: i64 = 24;
const CACHE_MAX_ENTRIES: usize = 10000;
const GOKZ_STATS_RATING_SOURCE: &str = "gokz_stats";

/// GOKZ 内存缓存条目类型
type GokzMemoryCache = HashMap<String, (GokzStats, DateTime<Utc>)>;
/// GOKZ 数据库缓存行类型（kzt/skz/vnl/ovr + 时间戳）
type GokzDbRow = (
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    DateTime<Utc>,
);
/// GOKZ 批量数据库行类型（含 steamid64）
type GokzBatchRow = (
    String,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
);

/// 统一 GOKZ 缓存管理器
/// 使用 PostgreSQL 作为持久化缓存，同时维护内存缓存加速读取
pub struct GokzCacheManager {
    /// 内存缓存：用于加速热点数据读取
    memory_cache: Arc<RwLock<GokzMemoryCache>>,
    db: Database,
}

impl GokzCacheManager {
    pub fn new(db: Database) -> Self {
        Self {
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }

    /// 从缓存获取 GOKZ 统计数据（先查内存，再查 PG）
    pub async fn get(&self, steamid64: &str) -> Option<GokzStats> {
        // 1. 尝试从内存缓存获取
        {
            let cache = self.memory_cache.read().await;
            if let Some((stats, ts)) = cache.get(steamid64) {
                if Utc::now() - *ts < Duration::hours(CACHE_TTL_HOURS) {
                    return Some(stats.clone());
                }
            }
        }

        // 2. 从 PG 缓存获取
        if let Some(stats) = self.get_from_db(steamid64).await {
            // 写入内存缓存
            let mut cache = self.memory_cache.write().await;
            cache.insert(steamid64.to_string(), (stats.clone(), Utc::now()));
            return Some(stats);
        }

        None
    }

    /// 从 PG 获取缓存
    async fn get_from_db(&self, steamid64: &str) -> Option<GokzStats> {
        let row: Option<GokzDbRow> = sqlx::query_as(
            r#"SELECT kzt_data, skz_data, vnl_data, ovr_data, expires_at
               FROM player_access_cache
               WHERE steamid64 = $1 AND rating_source = $2 AND expires_at > now()"#,
        )
        .bind(steamid64)
        .bind(GOKZ_STATS_RATING_SOURCE)
        .fetch_optional(&self.db.pool)
        .await
        .ok()?;

        let (kzt_data, skz_data, vnl_data, ovr_data, expires_at) = row?;

        // 检查是否过期
        if expires_at <= Utc::now() {
            return None;
        }

        Some(GokzStats {
            kzt: kzt_data.and_then(|v| serde_json::from_value(v).ok()),
            skz: skz_data.and_then(|v| serde_json::from_value(v).ok()),
            vnl: vnl_data.and_then(|v| serde_json::from_value(v).ok()),
            ovr: ovr_data.and_then(|v| serde_json::from_value(v).ok()),
        })
    }

    /// 写入缓存（同时写 PG 和内存）
    pub async fn set(&self, steamid64: &str, stats: &GokzStats) {
        let rating = stats
            .kzt
            .as_ref()
            .and_then(|k| k.rating)
            .map(|r| r as i32)
            .unwrap_or(0);
        let steam_level = 0i32; // steam_level 由 access_service 单独管理

        let kzt_json = stats
            .kzt
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok());
        let skz_json = stats
            .skz
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok());
        let vnl_json = stats
            .vnl
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok());
        let ovr_json = stats
            .ovr
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok());

        let expires_at = Utc::now() + Duration::hours(CACHE_TTL_HOURS);

        // 写入 PG
        let result = sqlx::query(
            r#"INSERT INTO player_access_cache (steamid64, rating, steam_level, rating_source, kzt_data, skz_data, vnl_data, ovr_data, expires_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
               ON CONFLICT (steamid64, rating_source) DO UPDATE
               SET rating = EXCLUDED.rating,
                   steam_level = EXCLUDED.steam_level,
                   kzt_data = EXCLUDED.kzt_data,
                   skz_data = EXCLUDED.skz_data,
                   vnl_data = EXCLUDED.vnl_data,
                   ovr_data = EXCLUDED.ovr_data,
                   expires_at = EXCLUDED.expires_at,
                   updated_at = now()"#,
        )
        .bind(steamid64)
        .bind(rating)
        .bind(steam_level)
        .bind(GOKZ_STATS_RATING_SOURCE)
        .bind(kzt_json)
        .bind(skz_json)
        .bind(vnl_json)
        .bind(ovr_json)
        .bind(expires_at)
        .execute(&self.db.pool)
        .await;

        match result {
            Ok(_) => {
                // 写入内存缓存
                let mut cache = self.memory_cache.write().await;
                cache.insert(steamid64.to_string(), (stats.clone(), Utc::now()));
                info!(steamid64, "GOKZ 缓存已写入");
            }
            Err(e) => {
                warn!(steamid64, error = %e, "GOKZ 缓存写入失败");
            }
        }
    }

    /// 批量获取多个玩家的 GOKZ 统计数据
    pub async fn get_batch(&self, steamids: &[String]) -> HashMap<String, GokzStats> {
        if steamids.is_empty() {
            return HashMap::new();
        }

        let mut results = HashMap::new();
        let mut uncached: Vec<&String> = Vec::new();

        // 1. 先查内存缓存
        {
            let cache = self.memory_cache.read().await;
            let now = Utc::now();
            for steamid64 in steamids {
                if let Some((stats, ts)) = cache.get(steamid64) {
                    if now - *ts < Duration::hours(CACHE_TTL_HOURS) {
                        results.insert(steamid64.clone(), stats.clone());
                        continue;
                    }
                }
                uncached.push(steamid64);
            }
        }

        if uncached.is_empty() {
            return results;
        }

        // 2. 从 PG 批量获取未命中内存的
        let rows: Vec<GokzBatchRow> = sqlx::query_as(
            r#"SELECT steamid64, kzt_data, skz_data, vnl_data, ovr_data
               FROM player_access_cache
               WHERE steamid64 = ANY($1) AND expires_at > now()"#,
        )
        .bind(steamids)
        .fetch_all(&self.db.pool)
        .await
        .unwrap_or_default();

        // 3. 写入内存缓存并收集结果
        let mut cache = self.memory_cache.write().await;
        let now = Utc::now();
        for (steamid64, kzt_data, skz_data, vnl_data, ovr_data) in rows {
            let stats = GokzStats {
                kzt: kzt_data.and_then(|v| serde_json::from_value(v).ok()),
                skz: skz_data.and_then(|v| serde_json::from_value(v).ok()),
                vnl: vnl_data.and_then(|v| serde_json::from_value(v).ok()),
                ovr: ovr_data.and_then(|v| serde_json::from_value(v).ok()),
            };
            cache.insert(steamid64.clone(), (stats.clone(), now));
            results.insert(steamid64, stats);
        }

        results
    }

    /// 清理过期缓存
    pub async fn cleanup(&self) -> anyhow::Result<u64> {
        let result = sqlx::query(r#"DELETE FROM player_access_cache WHERE expires_at < now()"#)
            .execute(&self.db.pool)
            .await?;

        // 清理内存缓存中的过期项
        let mut cache = self.memory_cache.write().await;
        let now = Utc::now();
        cache.retain(|_, (_, ts)| now - *ts < Duration::hours(CACHE_TTL_HOURS));

        // 如果内存缓存过大，清理最老的条目
        if cache.len() > CACHE_MAX_ENTRIES {
            let mut entries: Vec<_> = cache.iter().map(|(k, (_, ts))| (k.clone(), *ts)).collect();
            entries.sort_by_key(|(_, ts)| *ts);
            let to_remove = cache.len() - (CACHE_MAX_ENTRIES - 1000);
            for (k, _) in entries.into_iter().take(to_remove) {
                cache.remove(&k);
            }
        }

        let count = result.rows_affected();
        info!(count, "GOKZ 缓存清理完成");
        Ok(count)
    }

    /// 启动缓存清理定时任务
    pub fn start_cleanup_task(self: Arc<Self>, interval_secs: u64) {
        observability_service::register_task(
            "gokz_cache_cleanup",
            "GOKZ 缓存清理",
            "清理",
            Some(interval_secs),
            true,
        );
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                if let Err(e) = observability_service::observe_task(
                    "gokz_cache_cleanup",
                    self.cleanup(),
                    |count| format!("清理 {} 条 GOKZ 缓存", count),
                )
                .await
                {
                    warn!(error = %e, "GOKZ 缓存清理失败");
                }
            }
        });
    }
}
