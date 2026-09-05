mod a2s;
mod auth;
mod config;
mod cors_middleware;
mod db;
mod http_client;
mod models;
mod password;
mod rate_limit_middleware;
mod rcon;
mod request_log_middleware;
mod routes;
mod security_headers_middleware;
mod services;
mod sql_fragments;
#[cfg(test)]
mod test_util;

use axum::{extract::DefaultBodyLimit, http::StatusCode};
use config::Config;
use cors_middleware::CorsState;
use db::Database;
use rate_limit_middleware::RateLimitState;
use services::rate_limit_service::RateLimiters;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config = Config::from_env();
    http_client::init_http_client(config.http_timeout_secs, config.http_connect_timeout_secs)?;
    let db = Database::connect(&config.database_url, &config).await?;
    db.migrate().await?;
    db.seed(&config).await?;
    services::player_api_service::start_dispatch_loop(db.clone());
    let access_snapshot =
        services::access_snapshot_service::SnapshotStore::new("runtime/access_snapshot.json");
    services::access_snapshot_service::start_refresh_loop(db.clone(), access_snapshot.clone());
    // 启动封禁过期检查循环
    services::ban_expiry_service::start_expiry_loop(
        db.clone(),
        config.ban_expiry_check_interval_secs,
    );
    // 启动Steam名称定时刷新循环
    services::steam_name_refresh_service::start_steam_name_refresh_loop(
        db.clone(),
        config.clone(),
        config.steam_name_refresh_interval_secs,
    );
    // 启动过期 session 定时清理
    services::auth_service::start_session_cleanup_loop(
        db.clone(),
        config.session_cleanup_interval_secs,
    );
    // 启动外部服务器轮询，各服务器按自身 poll_interval 独立轮询
    services::rcon_poll_service::start_rcon_poll_loop(
        db.clone(),
        config.rcon_poll_scan_interval_secs,
    );
    // 启动休眠服务器 RCON 兑底轮询：空服休眠时插件无法上报，由后端通过 RCON 保持数据刷新
    if config.hibernation_poll_enabled {
        services::hibernation_poll_service::start_hibernation_poll_loop(
            db.clone(),
            services::hibernation_poll_service::HibernationPollConfig::from_config(&config),
        );
    }
    // 启动过期服务器状态清理，每 30 秒执行一次
    services::community_service::start_stale_cleanup_loop(db.clone());
    // 启动通知清理，每 24 小时清理 30 天前的已读通知
    services::notification_service::start_cleanup_loop(db.clone(), 86400);
    // 启动 LumiBot（QQ 机器人）事件上报队列同步（新白名单申请每 30 分钟集中上报）
    services::lumi_bot_service::start_sync_loop(db.clone(), config.clone());
    // 外部封禁同步采用持久化 outbox，业务请求只入队，由后台 worker 重试发送。
    services::external_ban_api_service::start_sync_loop(db.clone());
    // 启动服务器状态历史清理
    services::server_status_service::start_status_history_cleanup_loop(
        db.clone(),
        config.status_history_cleanup_interval_secs,
        config.status_history_retention_secs as i64,
    );
    // 启动进服记录清理
    services::access_log_service::start_access_log_cleanup_loop(
        db.clone(),
        config.access_log_cleanup_interval_secs,
        config.access_log_retention_days,
    );
    // 启动审计/操作日志/会话历史保留清理（默认每天一次）
    services::log_retention_service::start_log_retention_loop(db.clone(), 86400);
    // 启动全球封禁同步（从 KZTimer GlobalAPI）— 在 active_ban_cache 创建后调用
    // （移到 active_ban_cache 初始化之后）

    // 启动地图等级同步（如果配置了 MySQL），启动时同步一次，之后每 6 小时同步一次
    if let Some(ref mysql_url) = config.mysql_database_url {
        let sync = services::map_tier_service::MapTierSync::new(mysql_url.clone());
        sync.start_sync_loop(db.clone(), config.map_tier_sync_interval_secs);
    }
    // 启动服务器配置缓存
    let server_config_cache = Arc::new(services::server_config_cache::ServerConfigCache::new(
        config.server_config_cache_ttl_secs,
    ));
    services::server_config_cache::start_refresh_loop(
        db.clone(),
        server_config_cache.clone(),
        config.server_config_cache_refresh_interval_secs,
    );

    // 启动活跃封禁缓存
    let active_ban_cache = Arc::new(services::access_cache::ActiveBanCache::new());
    services::access_cache::start_ban_cache_refresh_loop(
        db.clone(),
        active_ban_cache.clone(),
        config.server_config_cache_refresh_interval_secs,
    );
    // 启动全球封禁同步（从 KZTimer GlobalAPI）
    services::global_ban_service::start_global_ban_sync_loop(
        db.clone(),
        active_ban_cache.clone(),
        config.global_ban_sync_interval_secs,
    );

    // 启动白名单缓存
    let whitelist_cache = Arc::new(services::access_cache::WhitelistCache::new());
    services::access_cache::start_whitelist_cache_refresh_loop(
        db.clone(),
        whitelist_cache.clone(),
        config.server_config_cache_refresh_interval_secs,
    );
    // 启动白名单低风险自动通过（低风险申请满 3 小时无人审核自动通过）
    services::whitelist_auto_approve_service::start_auto_approve_loop(
        db.clone(),
        whitelist_cache.clone(),
        config.clone(),
        60,
    );

    // 使用 PostgreSQL LISTEN/NOTIFY 立即刷新访问相关缓存；固定周期刷新作为兜底。
    services::access_cache::start_cache_invalidation_listener(
        db.clone(),
        access_snapshot.clone(),
        server_config_cache.clone(),
        active_ban_cache.clone(),
        whitelist_cache.clone(),
    );

    // 启动限流器
    let rate_limiters = Arc::new(RateLimiters::new());
    rate_limiters.clone().start_cleanup_task();
    let rate_limit_state = RateLimitState {
        limiters: rate_limiters,
    };

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    let max_body = config.max_request_body_bytes;
    let request_timeout = Duration::from_secs(config.request_timeout_secs);
    let cors_origins = config.cors_origins();
    let is_production = config.is_production;
    if cors_origins.is_empty() {
        tracing::warn!(
            "CORS_ORIGIN 未配置：管理后台仅允许同源访问，公开 /webhook/* 端点放行所有来源"
        );
    }
    let steam_resolver = services::steam_service::SteamResolver::new(&config);
    // 初始化统一 GOKZ 缓存管理器
    let gokz_cache = Arc::new(services::gokz_cache::GokzCacheManager::new(db.clone()));
    gokz_cache
        .clone()
        .start_cleanup_task(config.session_cleanup_interval_secs);

    let app = routes::router(
        config.clone(),
        db.clone(),
        access_snapshot.clone(),
        server_config_cache,
        active_ban_cache,
        whitelist_cache,
        steam_resolver,
        gokz_cache,
    )
    .layer(axum::middleware::from_fn(
        request_log_middleware::request_log_middleware,
    ))
    .layer(axum::middleware::from_fn_with_state(
        rate_limit_state,
        rate_limit_middleware::rate_limit_middleware,
    ))
    .layer(axum::middleware::from_fn_with_state(
        CorsState {
            allowed_origins: Arc::new(cors_origins),
        },
        cors_middleware::cors_middleware,
    ))
    .layer(axum::middleware::from_fn_with_state(
        security_headers_middleware::SecurityHeadersState {
            hsts_enabled: is_production,
        },
        security_headers_middleware::security_headers_middleware,
    ))
    .layer(
        ServiceBuilder::new()
            .layer(CompressionLayer::new().gzip(true))
            .layer(DefaultBodyLimit::max(max_body))
            .layer(RequestBodyLimitLayer::new(max_body))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                request_timeout,
            )),
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    tracing::info!("HTTP 服务已停止，刷写最终访问快照后退出");
    services::access_snapshot_service::shutdown_flush(&db, &access_snapshot).await;
    Ok(())
}

/// 阻塞直到收到 Ctrl+C 或 SIGTERM
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("收到退出信号，开始优雅关闭");
}
