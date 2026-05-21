use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::info;

/// 速率限制错误
#[derive(Debug)]
pub struct RateLimitError {
    /// 距离可以重试的秒数
    pub retry_after: u64,
    /// 限制名称（用于日志）
    pub limit_name: String,
}

/// 速率限制配置
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// 时间窗口内的最大请求数
    pub max_requests: u32,
    /// 时间窗口（秒）
    pub window_secs: u64,
    /// 限制名称
    pub name: String,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 60,
            window_secs: 60,
            name: "default".to_string(),
        }
    }
}

/// 限流条目
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

/// 基于 Key 的通用速率限制器
pub struct RateLimiter {
    entries: RwLock<HashMap<String, RateLimitEntry>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// 检查是否允许请求
    pub async fn check(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self.entries.write().await;
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_secs);

        let entry = entries.entry(key.to_string()).or_insert(RateLimitEntry {
            count: 0,
            window_start: now,
        });

        // 检查是否需要重置窗口
        if now.duration_since(entry.window_start) > window_duration {
            entry.count = 0;
            entry.window_start = now;
        }

        if entry.count >= self.config.max_requests {
            let elapsed = now.duration_since(entry.window_start).as_secs();
            let retry_after = self.config.window_secs.saturating_sub(elapsed);

            return Err(RateLimitError {
                retry_after,
                limit_name: self.config.name.clone(),
            });
        }

        entry.count += 1;
        Ok(())
    }

    /// 获取当前计数（用于监控）
    pub async fn get_count(&self, key: &str) -> u32 {
        let entries = self.entries.read().await;
        entries.get(key).map(|e| e.count).unwrap_or(0)
    }

    /// 清理过期条目
    pub async fn cleanup(&self) {
        let mut entries = self.entries.write().await;
        let now = Instant::now();
        let cleanup_threshold = Duration::from_secs(self.config.window_secs * 2);

        let before = entries.len();
        entries.retain(|_, entry| {
            now.duration_since(entry.window_start) < cleanup_threshold
        });
        let after = entries.len();

        if before != after {
            info!(
                name = %self.config.name,
                removed = before - after,
                remaining = after,
                "Rate limiter cleanup"
            );
        }
    }

    /// 获取统计信息
    pub async fn stats(&self) -> RateLimiterStats {
        let entries = self.entries.read().await;
        RateLimiterStats {
            total_keys: entries.len(),
            config: self.config.clone(),
        }
    }
}

#[derive(Debug)]
pub struct RateLimiterStats {
    pub total_keys: usize,
    pub config: RateLimitConfig,
}

/// 预定义的限流器配置
pub struct RateLimiters {
    /// 公开 API 限流器（按 IP）
    pub public_api: RateLimiter,
    /// 认证 API 限流器（按 IP，更严格）
    pub auth_api: RateLimiter,
    /// 插件 API 限流器（按 Token）
    pub plugin_api: RateLimiter,
    /// 管理 API 限流器（按用户）
    pub admin_api: RateLimiter,
}

impl RateLimiters {
    pub fn new() -> Self {
        Self {
            // 公开 API：每分钟 60 次/IP
            public_api: RateLimiter::new(RateLimitConfig {
                max_requests: 60,
                window_secs: 60,
                name: "public_api".to_string(),
            }),
            // 认证 API：每分钟 10 次/IP（防暴力破解）
            auth_api: RateLimiter::new(RateLimitConfig {
                max_requests: 10,
                window_secs: 60,
                name: "auth_api".to_string(),
            }),
            // 插件 API：每分钟 600 次/IP（支持 100+ 游戏服务器，每台每分钟多次请求）
            plugin_api: RateLimiter::new(RateLimitConfig {
                max_requests: 600,
                window_secs: 60,
                name: "plugin_api".to_string(),
            }),
            // 管理 API：每分钟 300 次/用户
            admin_api: RateLimiter::new(RateLimitConfig {
                max_requests: 300,
                window_secs: 60,
                name: "admin_api".to_string(),
            }),
        }
    }

    /// 启动定期清理任务
    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                self.public_api.cleanup().await;
                self.auth_api.cleanup().await;
                self.plugin_api.cleanup().await;
                self.admin_api.cleanup().await;
            }
        });
    }
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

/// 从请求中提取客户端 IP
pub fn extract_client_ip(headers: &axum::http::HeaderMap) -> String {
    // 尝试从 X-Forwarded-For 获取真实 IP
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(ip) = forwarded_str.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }

    // 尝试从 X-Real-IP 获取
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip) = real_ip.to_str() {
            return ip.to_string();
        }
    }

    // 默认返回 unknown（应该被反向代理设置）
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rate_limiter_allows_requests_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 5,
            window_secs: 60,
            name: "test".to_string(),
        });

        // 应该允许 5 次请求
        for _ in 0..5 {
            assert!(limiter.check("test_key").await.is_ok());
        }

        // 第 6 次应该被拒绝
        assert!(limiter.check("test_key").await.is_err());
    }

    #[tokio::test]
    async fn rate_limiter_returns_retry_after() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 1,
            window_secs: 60,
            name: "test".to_string(),
        });

        limiter.check("test_key").await.unwrap();
        let err = limiter.check("test_key").await.unwrap_err();

        assert!(err.retry_after > 0);
        assert!(err.retry_after <= 60);
    }
}