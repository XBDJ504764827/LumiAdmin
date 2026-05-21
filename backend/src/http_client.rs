use once_cell::sync::Lazy;
use reqwest::Client;
use std::time::Duration;

/// 全局 HTTP 客户端，复用连接池
pub static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(5))
        .user_agent("MangerBackend/1.0")
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(60))
        .build()
        .expect("Failed to create HTTP client")
});

/// 创建自定义配置的 HTTP 客户端
pub fn create_client(timeout_secs: u64, connect_timeout_secs: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .user_agent("MangerBackend/1.0")
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_else(|_| Client::new())
}