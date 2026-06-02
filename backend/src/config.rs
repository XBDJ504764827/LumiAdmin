#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub session_ttl_hours: i64,
    pub dev_username: String,
    pub dev_password: String,
    pub steam_api_key: Option<String>,
    pub steam_web_key: Option<String>,
    pub steamchina_profile_key: Option<String>,
    pub steamchina_level_key: Option<String>,
    pub max_request_body_bytes: usize,
    // 数据库连接池配置
    pub db_max_connections: u32,
    pub db_min_connections: u32,
    pub db_acquire_timeout_secs: u64,
    pub db_idle_timeout_secs: u64,
    // HTTP 客户端配置
    pub http_timeout_secs: u64,
    pub http_connect_timeout_secs: u64,
    // 请求超时
    pub request_timeout_secs: u64,
    // CORS 允许的来源
    pub cors_origin: Option<String>,
    // MySQL 数据库（地图等级同步）
    pub mysql_database_url: Option<String>,
    // 后台任务间隔配置
    pub ban_expiry_check_interval_secs: u64,
    pub steam_name_refresh_interval_secs: u64,
    pub session_cleanup_interval_secs: u64,
    pub rcon_poll_scan_interval_secs: u64,
    pub map_sync_scan_interval_secs: u64,
    pub map_tier_sync_interval_secs: u64,
    pub server_config_cache_ttl_secs: u64,
    pub server_config_cache_refresh_interval_secs: u64,
    // Cloudflare R2 存储配置
    pub r2_endpoint: Option<String>,
    pub r2_bucket: Option<String>,
    pub r2_access_key_id: Option<String>,
    pub r2_secret_access_key: Option<String>,
    pub r2_custom_domain: Option<String>,
    pub appeal_file_max_size_bytes: usize,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        let steam_api_key = std::env::var("STEAM_API_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let steam_web_key = std::env::var("STEAM_WEB_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| steam_api_key.clone());
        let steamchina_profile_key = std::env::var("STEAMCHINA_PROFILE_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let steamchina_level_key = std::env::var("STEAMCHINA_LEVEL_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let appeal_file_max_size_bytes = std::env::var("APPEAL_FILE_MAX_SIZE_MB")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100)
            * 1024
            * 1024;

        let max_request_body_bytes = std::env::var("MAX_REQUEST_BODY_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(appeal_file_max_size_bytes + 10 * 1024 * 1024);

        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL is required"),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3001".into()),
            session_ttl_hours: std::env::var("SESSION_TTL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(24),
            dev_username: std::env::var("DEV_USERNAME").unwrap_or_else(|_| "dev".into()),
            dev_password: std::env::var("DEV_PASSWORD").unwrap_or_else(|_| {
                tracing::warn!("DEV_PASSWORD 未设置，使用不安全的默认密码，请在生产环境中配置");
                "dev123".into()
            }),
            steam_api_key,
            steam_web_key,
            steamchina_profile_key,
            steamchina_level_key,
            max_request_body_bytes,
            // 数据库连接池配置
            db_max_connections: std::env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
            db_min_connections: std::env::var("DB_MIN_CONNECTIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            db_acquire_timeout_secs: std::env::var("DB_ACQUIRE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            db_idle_timeout_secs: std::env::var("DB_IDLE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(600),
            // HTTP 客户端配置
            http_timeout_secs: std::env::var("HTTP_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            http_connect_timeout_secs: std::env::var("HTTP_CONNECT_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            // 请求超时
            request_timeout_secs: std::env::var("REQUEST_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            // CORS 允许的来源
            cors_origin: std::env::var("CORS_ORIGIN").ok().filter(|v| !v.is_empty()),
            mysql_database_url: std::env::var("MYSQL_DATABASE_URL")
                .ok()
                .filter(|v| !v.is_empty()),
            // 后台任务间隔配置
            ban_expiry_check_interval_secs: env_u64("BAN_EXPIRY_CHECK_INTERVAL_SECS", 60),
            steam_name_refresh_interval_secs: env_u64("STEAM_NAME_REFRESH_INTERVAL_SECS", 6 * 3600),
            session_cleanup_interval_secs: env_u64("SESSION_CLEANUP_INTERVAL_SECS", 600),
            rcon_poll_scan_interval_secs: env_u64("RCON_POLL_SCAN_INTERVAL_SECS", 5),
            map_sync_scan_interval_secs: env_u64("MAP_SYNC_SCAN_INTERVAL_SECS", 60),
            map_tier_sync_interval_secs: env_u64("MAP_TIER_SYNC_INTERVAL_SECS", 6 * 3600),
            server_config_cache_ttl_secs: env_u64("SERVER_CONFIG_CACHE_TTL_SECS", 300),
            server_config_cache_refresh_interval_secs: env_u64(
                "SERVER_CONFIG_CACHE_REFRESH_INTERVAL_SECS",
                300,
            ),
            // R2 配置
            r2_endpoint: std::env::var("R2_ENDPOINT").ok().filter(|v| !v.is_empty()),
            r2_bucket: std::env::var("R2_BUCKET").ok().filter(|v| !v.is_empty()),
            r2_access_key_id: std::env::var("R2_ACCESS_KEY_ID")
                .ok()
                .filter(|v| !v.is_empty()),
            r2_secret_access_key: std::env::var("R2_SECRET_ACCESS_KEY")
                .ok()
                .filter(|v| !v.is_empty()),
            r2_custom_domain: std::env::var("R2_CUSTOM_DOMAIN")
                .ok()
                .filter(|v| !v.is_empty()),
            appeal_file_max_size_bytes,
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
