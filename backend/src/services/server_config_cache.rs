use crate::db::Database;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

/// 服务器配置缓存项
#[derive(Debug, Clone)]
pub struct CachedServerConfig {
    pub id: Uuid,
    pub community_id: Uuid,
    pub name: String,
    pub port: i32,
    pub report_token: String,
    pub access_restriction_enabled: bool,
    pub min_rating: i32,
    pub min_steam_level: i32,
    pub whitelist_mode_enabled: bool,
    pub use_custom_access: bool,
    pub community_whitelist_mode_enabled: bool,
    pub community_min_rating: i32,
    pub community_min_steam_level: i32,
}

impl CachedServerConfig {
    pub fn effective_min_rating(&self) -> i32 {
        if self.use_custom_access {
            self.min_rating
        } else {
            self.community_min_rating
        }
    }

    pub fn effective_min_steam_level(&self) -> i32 {
        if self.use_custom_access {
            self.min_steam_level
        } else {
            self.community_min_steam_level
        }
    }

    pub fn effective_access_restriction_enabled(&self) -> bool {
        if self.use_custom_access {
            self.access_restriction_enabled
        } else {
            self.community_min_rating > 0 || self.community_min_steam_level > 0
        }
    }

    pub fn effective_whitelist_mode_enabled(&self) -> bool {
        if self.use_custom_access {
            self.whitelist_mode_enabled
        } else {
            self.community_whitelist_mode_enabled
        }
    }
}

/// 服务器配置缓存
///
/// 用于缓存服务器配置，减少数据库查询次数。
/// 缓存会在后台定期刷新，同时支持手动失效。
pub struct ServerConfigCache {
    /// 按 (report_token, port) 索引的缓存
    by_token_port: RwLock<HashMap<(String, i32), CachedServerConfig>>,
    /// 按 server_id 索引的缓存
    by_id: RwLock<HashMap<Uuid, CachedServerConfig>>,
    /// 上次刷新时间
    last_refresh: RwLock<Instant>,
    /// 缓存有效期
    cache_ttl: Duration,
}

impl ServerConfigCache {
    /// 创建新的服务器配置缓存
    pub fn new(cache_ttl_secs: u64) -> Self {
        Self {
            by_token_port: RwLock::new(HashMap::new()),
            by_id: RwLock::new(HashMap::new()),
            last_refresh: RwLock::new(Instant::now() - Duration::from_secs(86400)), // 初始化为过期状态
            cache_ttl: Duration::from_secs(cache_ttl_secs),
        }
    }

    /// 根据 report_token 和 port 获取服务器配置
    pub async fn get_by_token_port(
        &self,
        db: &Database,
        report_token: &str,
        port: i32,
    ) -> anyhow::Result<Option<CachedServerConfig>> {
        let key = (report_token.to_string(), port);

        // 尝试从缓存获取
        {
            let cache = self.by_token_port.read().await;
            if let Some(config) = cache.get(&key) {
                return Ok(Some(config.clone()));
            }
        }

        // 缓存未命中，从数据库加载
        let config = self.load_server_config(db, report_token, port).await?;
        if let Some(ref cfg) = config {
            // 更新缓存
            let mut cache_by_token = self.by_token_port.write().await;
            let mut cache_by_id = self.by_id.write().await;
            cache_by_token.insert(key, cfg.clone());
            cache_by_id.insert(cfg.id, cfg.clone());
        }

        Ok(config)
    }

    /// 根据 server_id 获取服务器配置
    pub async fn get_by_id(
        &self,
        db: &Database,
        server_id: Uuid,
    ) -> anyhow::Result<Option<CachedServerConfig>> {
        // 尝试从缓存获取
        {
            let cache = self.by_id.read().await;
            if let Some(config) = cache.get(&server_id) {
                return Ok(Some(config.clone()));
            }
        }

        // 缓存未命中，从数据库加载
        let config = self.load_server_config_by_id(db, server_id).await?;
        if let Some(ref cfg) = config {
            // 更新缓存
            let mut cache_by_token = self.by_token_port.write().await;
            let mut cache_by_id = self.by_id.write().await;
            cache_by_token.insert((cfg.report_token.clone(), cfg.port), cfg.clone());
            cache_by_id.insert(cfg.id, cfg.clone());
        }

        Ok(config)
    }

    /// 刷新缓存（后台任务调用）
    pub async fn refresh(&self, db: &Database) -> anyhow::Result<()> {
        let configs = self.load_all_server_configs(db).await?;

        let mut cache_by_token = self.by_token_port.write().await;
        let mut cache_by_id = self.by_id.write().await;

        cache_by_token.clear();
        cache_by_id.clear();

        for config in configs {
            cache_by_token.insert((config.report_token.clone(), config.port), config.clone());
            cache_by_id.insert(config.id, config.clone());
        }

        let mut last_refresh = self.last_refresh.write().await;
        *last_refresh = Instant::now();

        info!(count = cache_by_id.len(), "服务器配置缓存已刷新");
        Ok(())
    }

    /// 使指定服务器的缓存失效
    pub async fn invalidate(&self, server_id: Uuid) {
        let mut cache_by_token = self.by_token_port.write().await;
        let mut cache_by_id = self.by_id.write().await;

        if let Some(config) = cache_by_id.remove(&server_id) {
            cache_by_token.remove(&(config.report_token, config.port));
        }
    }

    /// 使所有缓存失效
    pub async fn invalidate_all(&self) {
        let mut cache_by_token = self.by_token_port.write().await;
        let mut cache_by_id = self.by_id.write().await;

        cache_by_token.clear();
        cache_by_id.clear();
    }

    /// 检查缓存是否需要刷新
    pub async fn needs_refresh(&self) -> bool {
        let last_refresh = self.last_refresh.read().await;
        last_refresh.elapsed() > self.cache_ttl
    }

    /// 从数据库加载单个服务器配置
    async fn load_server_config(
        &self,
        db: &Database,
        report_token: &str,
        port: i32,
    ) -> anyhow::Result<Option<CachedServerConfig>> {
        let row: Option<(Uuid, Uuid, String, i32, String, bool, i32, i32, bool, bool, bool, i32, i32)> = sqlx::query_as(
            r#"SELECT s.id, s.community_id, s.name, s.port, s.report_token,
                      s.access_restriction_enabled, s.min_rating, s.min_steam_level, s.whitelist_mode_enabled,
                      s.use_custom_access, c.whitelist_mode_enabled, c.min_rating, c.min_steam_level
               FROM servers s
               JOIN communities c ON c.id = s.community_id
               WHERE s.report_token = $1 AND s.port = $2"#,
        )
        .bind(report_token)
        .bind(port)
        .fetch_optional(&db.pool)
        .await?;

        Ok(row.map(|(id, community_id, name, port, report_token, access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled, use_custom_access, community_whitelist_mode_enabled, community_min_rating, community_min_level)| {
            CachedServerConfig {
                id,
                community_id,
                name,
                port,
                report_token,
                access_restriction_enabled,
                min_rating,
                min_steam_level,
                whitelist_mode_enabled,
                use_custom_access,
                community_whitelist_mode_enabled,
                community_min_rating,
                community_min_steam_level: community_min_level,
            }
        }))
    }

    /// 根据 ID 加载服务器配置
    async fn load_server_config_by_id(
        &self,
        db: &Database,
        server_id: Uuid,
    ) -> anyhow::Result<Option<CachedServerConfig>> {
        let row: Option<(Uuid, Uuid, String, i32, String, bool, i32, i32, bool, bool, bool, i32, i32)> = sqlx::query_as(
            r#"SELECT s.id, s.community_id, s.name, s.port, s.report_token,
                      s.access_restriction_enabled, s.min_rating, s.min_steam_level, s.whitelist_mode_enabled,
                      s.use_custom_access, c.whitelist_mode_enabled, c.min_rating, c.min_steam_level
               FROM servers s
               JOIN communities c ON c.id = s.community_id
               WHERE s.id = $1"#,
        )
        .bind(server_id)
        .fetch_optional(&db.pool)
        .await?;

        Ok(row.map(|(id, community_id, name, port, report_token, access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled, use_custom_access, community_whitelist_mode_enabled, community_min_rating, community_min_level)| {
            CachedServerConfig {
                id,
                community_id,
                name,
                port,
                report_token,
                access_restriction_enabled,
                min_rating,
                min_steam_level,
                whitelist_mode_enabled,
                use_custom_access,
                community_whitelist_mode_enabled,
                community_min_rating,
                community_min_steam_level: community_min_level,
            }
        }))
    }

    /// 从数据库加载所有服务器配置
    async fn load_all_server_configs(&self, db: &Database) -> anyhow::Result<Vec<CachedServerConfig>> {
        let rows: Vec<(Uuid, Uuid, String, i32, String, bool, i32, i32, bool, bool, bool, i32, i32)> = sqlx::query_as(
            r#"SELECT s.id, s.community_id, s.name, s.port, s.report_token,
                      s.access_restriction_enabled, s.min_rating, s.min_steam_level, s.whitelist_mode_enabled,
                      s.use_custom_access, c.whitelist_mode_enabled, c.min_rating, c.min_steam_level
               FROM servers s
               JOIN communities c ON c.id = s.community_id
               WHERE s.report_token IS NOT NULL AND s.report_token != ''"#,
        )
        .fetch_all(&db.pool)
        .await?;

        Ok(rows.into_iter().map(|(id, community_id, name, port, report_token, access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled, use_custom_access, community_whitelist_mode_enabled, community_min_rating, community_min_level)| {
            CachedServerConfig {
                id,
                community_id,
                name,
                port,
                report_token,
                access_restriction_enabled,
                min_rating,
                min_steam_level,
                whitelist_mode_enabled,
                use_custom_access,
                community_whitelist_mode_enabled,
                community_min_rating,
                community_min_steam_level: community_min_level,
            }
        }).collect())
    }
}

/// 启动服务器配置缓存刷新循环
pub fn start_refresh_loop(db: Database, cache: Arc<ServerConfigCache>, interval_secs: u64) {
    tokio::spawn(async move {
        // 首次启动立即刷新
        if let Err(error) = cache.refresh(&db).await {
            tracing::warn!(%error, "首次刷新服务器配置缓存失败");
        }

        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            if let Err(error) = cache.refresh(&db).await {
                tracing::warn!(%error, "刷新服务器配置缓存失败");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_ttl_check_works() {
        let cache = ServerConfigCache::new(60);

        // 刚创建时应该需要刷新
        assert!(tokio_test::block_on(cache.needs_refresh()));
    }
}