use reqwest::Client;
use std::sync::OnceLock;
use std::time::Duration;

static HTTP_CLIENT_INNER: OnceLock<Client> = OnceLock::new();

/// 获取全局 HTTP 客户端
pub fn http_client() -> &'static Client {
    HTTP_CLIENT_INNER.get_or_init(|| {
        // 降级默认值（init_http_client 未显式调用时使用）
        Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("MangerBackend/1.0")
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client")
    })
}

/// 使用 Config 中的超时配置初始化全局 HTTP 客户端（必须在首次使用前调用）
pub fn init_http_client(timeout_secs: u64, connect_timeout_secs: u64) -> anyhow::Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .user_agent("MangerBackend/1.0")
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| anyhow::anyhow!("HTTP 客户端初始化失败: {e}"))?;
    let _ = HTTP_CLIENT_INNER.set(client);
    Ok(())
}
