#[derive(Clone, Debug)]
pub struct Config {
    pub app_env: String,
    pub is_production: bool,
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
    pub map_tier_sync_interval_secs: u64,
    // 服务器状态历史清理
    pub status_history_cleanup_interval_secs: u64,
    pub status_history_retention_secs: u64,
    pub server_config_cache_ttl_secs: u64,
    pub server_config_cache_refresh_interval_secs: u64,
    // 进服记录清理
    pub access_log_cleanup_interval_secs: u64,
    pub access_log_retention_days: i64,
    // 全球封禁同步
    pub global_ban_sync_interval_secs: u64,
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

        let appeal_file_max_size_bytes = match std::env::var("APPEAL_FILE_MAX_SIZE_MB") {
            Ok(val) => match val.parse::<usize>() {
                Ok(mb) => mb.max(1) * 1024 * 1024,
                Err(_) => {
                    tracing::warn!(value = %val, "APPEAL_FILE_MAX_SIZE_MB 解析失败，使用默认值 100MB");
                    100 * 1024 * 1024
                }
            },
            Err(_) => 100 * 1024 * 1024,
        };

        let max_request_body_bytes = std::env::var("MAX_REQUEST_BODY_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(appeal_file_max_size_bytes + 10 * 1024 * 1024);

        let app_env = std::env::var("APP_ENV")
            .unwrap_or_else(|_| "development".into())
            .trim()
            .to_lowercase();
        let is_production = app_env == "production";

        let dev_password = match std::env::var("DEV_PASSWORD") {
            Ok(v) if !v.is_empty() => v,
            _ if is_production => {
                tracing::error!("生产环境必须设置 DEV_PASSWORD 环境变量");
                std::process::exit(1);
            }
            _ => {
                tracing::warn!("DEV_PASSWORD 未设置，使用不安全的默认密码，请在生产环境中配置");
                "change-me".into()
            }
        };

        if matches!(dev_password.as_str(), "change-me" | "dev123") {
            if is_production {
                tracing::error!("生产环境禁止使用默认 DEV_PASSWORD，请设置高强度初始管理员密码");
                std::process::exit(1);
            }
            tracing::warn!("当前 DEV_PASSWORD 是默认示例值，请勿用于生产环境");
        }

        let mut config = Self {
            app_env,
            is_production,
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL is required"),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3001".into()),
            session_ttl_hours: env_u64("SESSION_TTL_HOURS", 24) as i64,
            dev_username: std::env::var("DEV_USERNAME").unwrap_or_else(|_| "dev".into()),
            dev_password,
            steam_api_key,
            steam_web_key,
            steamchina_profile_key,
            steamchina_level_key,
            max_request_body_bytes,
            // 数据库连接池配置
            db_max_connections: env_u32_clamped("DB_MAX_CONNECTIONS", 20, 1, 100),
            db_min_connections: env_u32_clamped("DB_MIN_CONNECTIONS", 2, 0, 50),
            db_acquire_timeout_secs: env_u64("DB_ACQUIRE_TIMEOUT_SECS", 10),
            db_idle_timeout_secs: env_u64("DB_IDLE_TIMEOUT_SECS", 600),
            // HTTP 客户端配置
            http_timeout_secs: env_u64("HTTP_TIMEOUT_SECS", 300),
            http_connect_timeout_secs: env_u64("HTTP_CONNECT_TIMEOUT_SECS", 5),
            // 请求超时
            request_timeout_secs: env_u64("REQUEST_TIMEOUT_SECS", 300),
            // CORS 允许的来源
            cors_origin: std::env::var("CORS_ORIGIN").ok().filter(|v| !v.is_empty()),
            mysql_database_url: std::env::var("MYSQL_DATABASE_URL")
                .ok()
                .filter(|v| !v.is_empty()),
            // 后台任务间隔配置
            ban_expiry_check_interval_secs: env_u64("BAN_EXPIRY_CHECK_INTERVAL_SECS", 15),
            steam_name_refresh_interval_secs: env_u64("STEAM_NAME_REFRESH_INTERVAL_SECS", 6 * 3600),
            session_cleanup_interval_secs: env_u64("SESSION_CLEANUP_INTERVAL_SECS", 600),
            rcon_poll_scan_interval_secs: env_u64("RCON_POLL_SCAN_INTERVAL_SECS", 5),
            map_tier_sync_interval_secs: env_u64("MAP_TIER_SYNC_INTERVAL_SECS", 6 * 3600),
            // 服务器状态历史清理：每小时清理一次，保留 1 小时数据
            status_history_cleanup_interval_secs: env_u64(
                "STATUS_HISTORY_CLEANUP_INTERVAL_SECS",
                3600,
            ),
            status_history_retention_secs: env_u64("STATUS_HISTORY_RETENTION_SECS", 3600),
            server_config_cache_ttl_secs: env_u64("SERVER_CONFIG_CACHE_TTL_SECS", 300),
            server_config_cache_refresh_interval_secs: env_u64(
                "SERVER_CONFIG_CACHE_REFRESH_INTERVAL_SECS",
                300,
            ),
            // 进服记录清理：默认每天清理一次，保留 90 天
            access_log_cleanup_interval_secs: env_u64("ACCESS_LOG_CLEANUP_INTERVAL_SECS", 86400),
            access_log_retention_days: std::env::var("ACCESS_LOG_RETENTION_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .filter(|v: &i64| *v > 0)
                .unwrap_or(90),
            // 全球封禁同步：默认 5 分钟
            global_ban_sync_interval_secs: env_u64("GLOBAL_BAN_SYNC_INTERVAL_SECS", 300),
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
        };

        // 跨字段校验
        if config.db_min_connections > config.db_max_connections {
            tracing::warn!(
                min = config.db_min_connections,
                max = config.db_max_connections,
                "DB_MIN_CONNECTIONS 大于 DB_MAX_CONNECTIONS，已自动修正为 DB_MAX_CONNECTIONS"
            );
            config.db_min_connections = config.db_max_connections;
        }

        if config.db_acquire_timeout_secs == 0 {
            tracing::warn!("DB_ACQUIRE_TIMEOUT_SECS 为 0，已自动修正为 10");
            config.db_acquire_timeout_secs = 10;
        }

        if config.http_connect_timeout_secs > config.http_timeout_secs {
            tracing::warn!(
                connect = config.http_connect_timeout_secs,
                timeout = config.http_timeout_secs,
                "HTTP_CONNECT_TIMEOUT_SECS 大于 HTTP_TIMEOUT_SECS，已自动修正"
            );
            config.http_connect_timeout_secs = config.http_timeout_secs;
        }

        if config.request_timeout_secs == 0 {
            tracing::warn!("REQUEST_TIMEOUT_SECS 为 0，已自动修正为 300");
            config.request_timeout_secs = 300;
        }

        if config.http_timeout_secs == 0 {
            tracing::warn!("HTTP_TIMEOUT_SECS 为 0，已自动修正为 300");
            config.http_timeout_secs = 300;
        }

        if config.max_request_body_bytes <= config.appeal_file_max_size_bytes {
            let corrected = config.appeal_file_max_size_bytes + 10 * 1024 * 1024;
            tracing::warn!(
                max_request_body_bytes = config.max_request_body_bytes,
                appeal_file_max_size_bytes = config.appeal_file_max_size_bytes,
                corrected,
                "MAX_REQUEST_BODY_BYTES 必须大于 APPEAL_FILE_MAX_SIZE_MB，已自动修正"
            );
            config.max_request_body_bytes = corrected;
        }

        if config.is_production && config.cors_origin.is_none() {
            tracing::error!("生产环境必须设置 CORS_ORIGIN，避免后台 API 被任意来源调用");
            std::process::exit(1);
        }

        // R2 凭据完整性检查
        let r2_count = [
            config.r2_endpoint.is_some(),
            config.r2_bucket.is_some(),
            config.r2_access_key_id.is_some(),
            config.r2_secret_access_key.is_some(),
        ]
        .iter()
        .filter(|&&v| v)
        .count();
        if r2_count > 0 && r2_count < 4 {
            if config.is_production {
                tracing::error!(
                    filled = r2_count,
                    total = 4,
                    "生产环境 R2 存储配置不完整，请补齐或清空 R2_* 配置"
                );
                std::process::exit(1);
            }
            tracing::warn!(
                filled = r2_count,
                total = 4,
                "R2 存储配置不完整，部分字段已填写但缺少其他字段，R2 功能将不可用"
            );
        }

        // 启动配置日志（隐藏敏感字段）
        tracing::info!(
            bind_addr = %config.bind_addr,
            db_max_connections = config.db_max_connections,
            db_min_connections = config.db_min_connections,
            session_ttl_hours = config.session_ttl_hours,
            request_timeout_secs = config.request_timeout_secs,
            app_env = %config.app_env,
            is_production = config.is_production,
            r2_enabled = config.r2_storage_enabled(),
            "应用配置已加载"
        );

        config
    }

    pub fn r2_storage_enabled(&self) -> bool {
        self.r2_endpoint.is_some()
            && self.r2_bucket.is_some()
            && self.r2_access_key_id.is_some()
            && self.r2_secret_access_key.is_some()
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    match std::env::var(key) {
        Ok(val) => match val.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                tracing::warn!(key = key, value = %val, default = default, "环境变量解析失败，使用默认值");
                default
            }
        },
        Err(_) => default,
    }
}

/// 带范围校验的 u32 环境变量读取
fn env_u32_clamped(key: &str, default: u32, min: u32, max: u32) -> u32 {
    match std::env::var(key) {
        Ok(val) => match val.parse::<u32>() {
            Ok(n) => {
                if n < min || n > max {
                    tracing::warn!(
                        key = key,
                        value = n,
                        min = min,
                        max = max,
                        "环境变量超出范围，已自动修正"
                    );
                }
                n.clamp(min, max)
            }
            Err(_) => {
                tracing::warn!(key = key, value = %val, default = default, "环境变量解析失败，使用默认值");
                default
            }
        },
        Err(_) => default,
    }
}
