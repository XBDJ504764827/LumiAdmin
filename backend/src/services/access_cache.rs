use crate::db::Database;
use crate::services::observability_service;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CachedBan {
    pub id: Uuid,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct ActiveBanCache {
    by_steam_id: RwLock<HashMap<String, CachedBan>>,
    by_ip: RwLock<HashMap<String, CachedBan>>,
}

impl ActiveBanCache {
    pub fn new() -> Self {
        Self {
            by_steam_id: RwLock::new(HashMap::new()),
            by_ip: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_by_steam_id(&self, steam_id: &str) -> Option<CachedBan> {
        let cache = self.by_steam_id.read().await;
        cache.get(steam_id).and_then(|ban| {
            if ban.expires_at.is_none_or(|exp| exp > Utc::now()) {
                Some(ban.clone())
            } else {
                None
            }
        })
    }

    pub async fn get_by_ip(&self, ip: &str) -> Option<CachedBan> {
        let cache = self.by_ip.read().await;
        cache.get(ip).and_then(|ban| {
            if ban.expires_at.is_none_or(|exp| exp > Utc::now()) {
                Some(ban.clone())
            } else {
                None
            }
        })
    }

    pub async fn refresh(&self, db: &Database) -> anyhow::Result<()> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(Option<String>, Option<String>, Uuid, String, Option<DateTime<Utc>>)> =
            sqlx::query_as(
                r#"SELECT steam_id, ip_address, id, reason, expires_at
                   FROM ban_records
                   WHERE status = 'active'
                     AND (expires_at IS NULL OR expires_at > now())"#,
            )
            .fetch_all(&db.pool)
            .await?;

        let mut by_steam_id = HashMap::new();
        let mut by_ip = HashMap::new();
        let now = Utc::now();

        for (steam_id, ip_address, id, reason, expires_at) in &rows {
            let expired = expires_at.is_some_and(|exp| exp <= now);
            if expired {
                continue;
            }
            let ban = CachedBan {
                id: *id,
                reason: reason.clone(),
                expires_at: *expires_at,
            };
            if let Some(sid) = steam_id {
                by_steam_id.insert(sid.clone(), ban.clone());
            }
            if let Some(ip) = ip_address {
                by_ip.insert(ip.clone(), ban);
            }
        }

        let steam_count = by_steam_id.len();
        let ip_count = by_ip.len();
        *self.by_steam_id.write().await = by_steam_id;
        *self.by_ip.write().await = by_ip;

        info!(steam_count, ip_count, "活跃封禁缓存已刷新");
        Ok(())
    }
}

pub struct WhitelistCache {
    approved: RwLock<HashSet<String>>,
}

impl WhitelistCache {
    pub fn new() -> Self {
        Self {
            approved: RwLock::new(HashSet::new()),
        }
    }

    pub async fn contains(&self, steamid64: &str) -> bool {
        self.approved.read().await.contains(steamid64)
    }

    pub async fn refresh(&self, db: &Database) -> anyhow::Result<()> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"SELECT steamid64 FROM whitelist_requests WHERE status = 'approved'"#,
        )
        .fetch_all(&db.pool)
        .await?;

        let approved: HashSet<String> = rows.into_iter().map(|(s,)| s).collect();
        let count = approved.len();
        *self.approved.write().await = approved;

        info!(count, "白名单缓存已刷新");
        Ok(())
    }
}

pub fn start_ban_cache_refresh_loop(db: Database, cache: Arc<ActiveBanCache>, interval_secs: u64) {
    observability_service::register_task(
        "active_ban_cache_refresh",
        "活跃封禁缓存刷新",
        "缓存",
        Some(interval_secs),
        true,
    );
    tokio::spawn(async move {
        if let Err(error) = observability_service::observe_task(
            "active_ban_cache_refresh",
            cache.refresh(&db),
            |_| "初始封禁缓存刷新完成".to_string(),
        )
        .await
        {
            tracing::warn!(%error, "首次刷新活跃封禁缓存失败");
        }
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            if let Err(error) = observability_service::observe_task(
                "active_ban_cache_refresh",
                cache.refresh(&db),
                |_| "封禁缓存刷新完成".to_string(),
            )
            .await
            {
                tracing::warn!(%error, "刷新活跃封禁缓存失败");
            }
        }
    });
}

pub fn start_whitelist_cache_refresh_loop(
    db: Database,
    cache: Arc<WhitelistCache>,
    interval_secs: u64,
) {
    observability_service::register_task(
        "whitelist_cache_refresh",
        "白名单缓存刷新",
        "缓存",
        Some(interval_secs),
        true,
    );
    tokio::spawn(async move {
        if let Err(error) = observability_service::observe_task(
            "whitelist_cache_refresh",
            cache.refresh(&db),
            |_| "初始白名单缓存刷新完成".to_string(),
        )
        .await
        {
            tracing::warn!(%error, "首次刷新白名单缓存失败");
        }
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            if let Err(error) = observability_service::observe_task(
                "whitelist_cache_refresh",
                cache.refresh(&db),
                |_| "白名单缓存刷新完成".to_string(),
            )
            .await
            {
                tracing::warn!(%error, "刷新白名单缓存失败");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn active_ban_cache_returns_none_when_empty() {
        let cache = ActiveBanCache::new();
        assert!(cache.get_by_steam_id("76561198000000001").await.is_none());
        assert!(cache.get_by_ip("1.2.3.4").await.is_none());
    }

    #[tokio::test]
    async fn whitelist_cache_returns_false_when_empty() {
        let cache = WhitelistCache::new();
        assert!(!cache.contains("76561198000000001").await);
    }
}
