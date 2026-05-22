mod a2s;
mod auth;
mod config;
mod db;
mod http_client;
mod models;
mod password;
mod rate_limit_middleware;
mod rcon;
mod routes;
mod services;

use axum::http::{Method, StatusCode, header};
use config::Config;
use db::Database;
use rate_limit_middleware::RateLimitState;
use services::rate_limit_service::RateLimiters;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::compression::CompressionLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config = Config::from_env();
    http_client::init_http_client(config.http_timeout_secs, config.http_connect_timeout_secs);
    let db = Database::connect(&config.database_url, &config).await?;
    db.migrate().await?;
    db.seed(&config).await?;
    services::player_api_service::start_dispatch_loop(db.clone());
    let access_snapshot =
        services::access_snapshot_service::SnapshotStore::new("runtime/access_snapshot.json");
    services::access_snapshot_service::start_refresh_loop(db.clone(), access_snapshot.clone());
    // 启动封禁过期检查循环，每 60 秒检查一次
    services::ban_expiry_service::start_expiry_loop(db.clone(), 60);
    // 启动Steam名称定时刷新循环，每 6 小时刷新一次
    services::steam_name_refresh_service::start_steam_name_refresh_loop(db.clone(), config.clone(), 6 * 3600);
    // 启动过期 session 定时清理，每 10 分钟清理一次
    services::auth_service::start_session_cleanup_loop(db.clone(), 600);
    // 启动外部服务器轮询，每 5 秒扫描一次，各服务器按自身 poll_interval 独立轮询
    services::rcon_poll_service::start_rcon_poll_loop(db.clone(), 5);
    // 启动过期服务器状态清理，每 30 秒执行一次
    services::community_service::start_stale_cleanup_loop(db.clone());

    // 启动地图等级同步（如果配置了 MySQL），启动时同步一次，之后每 6 小时同步一次
    if let Some(ref mysql_url) = config.mysql_database_url {
        services::map_tier_service::start_sync_loop(db.clone(), mysql_url.clone(), 6 * 3600);
    }
    // 启动服务器配置缓存，每 5 分钟刷新一次
    let server_config_cache = Arc::new(services::server_config_cache::ServerConfigCache::new(300));
    services::server_config_cache::start_refresh_loop(db.clone(), server_config_cache.clone(), 300);

    // 启动限流器
    let rate_limiters = Arc::new(RateLimiters::new());
    rate_limiters.clone().start_cleanup_task();
    let rate_limit_state = RateLimitState {
        limiters: rate_limiters,
    };

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    let max_body = config.max_request_body_bytes;
    let request_timeout = Duration::from_secs(config.request_timeout_secs);
    let cors_origin = config.cors_origin.clone();
    let steam_resolver = services::steam_service::SteamResolver::new(&config);
    let app = routes::router(config, db, access_snapshot, server_config_cache, steam_resolver)
        .layer(axum::middleware::from_fn_with_state(
            rate_limit_state,
            rate_limit_middleware::rate_limit_middleware,
        ))
        .layer(
        ServiceBuilder::new()
            .layer(CompressionLayer::new().gzip(true))
            .layer(RequestBodyLimitLayer::new(max_body))
            .layer({
                let mut cors = CorsLayer::new()
                    .allow_methods([
                        Method::GET,
                        Method::POST,
                        Method::PUT,
                        Method::DELETE,
                        Method::OPTIONS,
                    ])
                    .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
                if let Some(origin) = cors_origin {
                    if let Ok(parsed) = origin.parse::<axum::http::HeaderValue>() {
                        cors = cors.allow_origin(parsed);
                    }
                } else {
                    tracing::warn!("CORS_ORIGIN 未配置，允许所有来源访问。生产环境请设置 CORS_ORIGIN 环境变量");
                    cors = cors.allow_origin(tower_http::cors::Any);
                }
                cors
            })
            .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, request_timeout)),
    );
    axum::serve(listener, app).await?;
    Ok(())
}
