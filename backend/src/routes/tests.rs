use crate::routes::router;
use crate::{
    config::Config,
    db::Database,
    services::{
        access_snapshot_service::SnapshotStore, server_config_cache::ServerConfigCache,
        steam_service::SteamResolver,
    },
};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tower::ServiceExt;
use uuid::Uuid;

fn test_snapshot_store() -> SnapshotStore {
    SnapshotStore::new(
        std::env::temp_dir().join(format!("manger-test-snapshot-{}.json", Uuid::new_v4())),
    )
}

fn test_app(config: Config, db: Database) -> Router {
    router(
        config.clone(),
        db,
        test_snapshot_store(),
        Arc::new(ServerConfigCache::new(300)),
        SteamResolver::new(&config),
    )
}

fn schema_url(base_url: &str, schema: &str) -> String {
    let separator = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{separator}options=-csearch_path%3D{schema}")
}

async fn create_schema(base_url: &str, schema: &str) {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(base_url)
        .await
        .unwrap();
    sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn drop_schema(base_url: &str, schema: &str) {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(base_url)
        .await
        .unwrap();
    sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn with_test_app(test: impl AsyncFnOnce(Database, Config) -> anyhow::Result<()>) {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);
    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;
        db.migrate().await?;
        db.seed(&config).await?;
        test(db, config).await
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

async fn ensure_test_user_exists(db: &Database, user_id: &str) -> anyhow::Result<()> {
    let exists: bool =
        sqlx::query_scalar::<_, bool>(r#"SELECT EXISTS(SELECT 1 FROM users WHERE id = $1::uuid)"#)
            .bind(user_id)
            .fetch_one(&db.pool)
            .await?;
    if exists {
        return Ok(());
    }

    let (username, display_name, role, password) = match user_id {
        "11111111-1111-1111-1111-111111111111" => ("alex", "Alex", "admin", "admin123"),
        "33333333-3333-3333-3333-333333333333" => ("james", "James", "normal", "normal123"),
        "22222222-2222-2222-2222-222222222222" => ("devadmin", "DevAdmin", "developer", "dev123"),
        _ => anyhow::bail!("unknown test user id: {user_id}"),
    };

    sqlx::query(
        r#"INSERT INTO users (id, username, display_name, password_hash, role)
               VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(Uuid::parse_str(user_id).unwrap_or(Uuid::new_v4()))
    .bind(username)
    .bind(display_name)
    .bind(password)
    .bind(role)
    .execute(&db.pool)
    .await?;

    Ok(())
}

async fn create_session_for_user(db: &Database, user_id: &str) -> anyhow::Result<Uuid> {
    ensure_test_user_exists(db, user_id).await?;

    let user = sqlx::query_as::<_, crate::models::User>(
            r#"SELECT id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at FROM users WHERE id = $1::uuid"#,
        )
        .bind(user_id)
        .fetch_one(&db.pool)
        .await?;

    let session = crate::auth::session::build_session(&user, 24);
    sqlx::query(
            r#"INSERT INTO sessions (token, user_id, role, display_name, role_label, expires_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(session.token)
        .bind(session.user_id)
        .bind(&session.role)
        .bind(&session.display_name)
        .bind(&session.role_label)
        .bind(session.expires_at)
        .bind(session.created_at)
        .execute(&db.pool)
        .await?;

    Ok(session.token)
}

async fn insert_whitelist(db: &Database, status: &str) -> Uuid {
    let id = Uuid::new_v4();
    let steamid64 = format!("7656119{:010}", id.as_u128() % 10_000_000_000);
    let steamid = format!("STEAM_0:1:{}", (id.as_u128() % 100000) as u64);
    let steamid3 = format!("[U:1:{}]", (id.as_u128() % 100000) as u64);
    let profile_url = format!("https://steamcommunity.com/profiles/{steamid64}");
    sqlx::query(
        r#"
            INSERT INTO whitelist_requests (
                id, steam_id, steamid64, steamid, steamid3, profile_url, nickname, status,
                applied_at, approved_at, approved_by, rejected_at, rejected_by,
                rejection_reason, source, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, '测试玩家', $7,
                    now(),
                    CASE WHEN $7 = 'approved' THEN now() ELSE NULL END,
                    CASE WHEN $7 = 'approved' THEN 'Alex' ELSE NULL END,
                    CASE WHEN $7 = 'rejected' THEN now() ELSE NULL END,
                    CASE WHEN $7 = 'rejected' THEN 'Alex' ELSE NULL END,
                    CASE WHEN $7 = 'rejected' THEN '资料不完整' ELSE NULL END,
                    'public', now())
            "#,
    )
    .bind(id)
    .bind(&steamid)
    .bind(&steamid64)
    .bind(&steamid)
    .bind(&steamid3)
    .bind(&profile_url)
    .bind(status)
    .execute(&db.pool)
    .await
    .unwrap();
    id
}

async fn insert_community_with_server(db: &Database, name: &str) -> (Uuid, Uuid) {
    let community_id = Uuid::new_v4();
    let server_id = Uuid::new_v4();

    sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
        .bind(community_id)
        .bind(name)
        .execute(&db.pool)
        .await
        .unwrap();

    sqlx::query(
            r#"
            INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'online', $8)
            "#,
        )
        .bind(server_id)
        .bind(community_id)
        .bind("一号服")
        .bind("127.0.0.1")
        .bind(25575_i32)
        .bind("secret")
        .bind("plugin-token")
        .bind(Vec::<String>::new())
        .execute(&db.pool)
        .await
        .unwrap();

    (community_id, server_id)
}

async fn spawn_fake_rcon_server() -> (u16, tokio::task::JoinHandle<anyhow::Result<()>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let (request_id, _, _) = read_rcon_packet(&mut stream).await?;
        write_rcon_packet(&mut stream, request_id, 0, "").await?;
        write_rcon_packet(&mut stream, request_id, 2, "").await?;
        let (request_id, _, _) = read_rcon_packet(&mut stream).await?;
        write_rcon_packet(&mut stream, request_id, 0, "玩家甲").await?;
        Ok(())
    });
    (port, handle)
}

async fn spawn_webhook_server(
    status: &'static str,
) -> (String, tokio::task::JoinHandle<anyhow::Result<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let mut buffer = vec![0_u8; 8192];
        let n = stream.read(&mut buffer).await?;
        let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
        let response =
            format!("HTTP/1.1 {status}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        stream.write_all(response.as_bytes()).await?;
        Ok(request)
    });
    (url, handle)
}

async fn read_rcon_packet(
    stream: &mut tokio::net::TcpStream,
) -> std::io::Result<(i32, i32, String)> {
    let mut size_bytes = [0_u8; 4];
    stream.read_exact(&mut size_bytes).await?;
    let size = i32::from_le_bytes(size_bytes);
    let mut payload = vec![0_u8; size as usize];
    stream.read_exact(&mut payload).await?;
    let mut request_id_bytes = [0_u8; 4];
    request_id_bytes.copy_from_slice(&payload[0..4]);
    let mut packet_type_bytes = [0_u8; 4];
    packet_type_bytes.copy_from_slice(&payload[4..8]);
    Ok((
        i32::from_le_bytes(request_id_bytes),
        i32::from_le_bytes(packet_type_bytes),
        String::from_utf8_lossy(&payload[8..payload.len() - 2]).into_owned(),
    ))
}

async fn write_rcon_packet(
    stream: &mut tokio::net::TcpStream,
    request_id: i32,
    packet_type: i32,
    body: &str,
) -> std::io::Result<()> {
    let size = body.len() + 10;
    let mut packet = Vec::with_capacity(size + 4);
    packet.extend_from_slice(&(size as i32).to_le_bytes());
    packet.extend_from_slice(&request_id.to_le_bytes());
    packet.extend_from_slice(&packet_type.to_le_bytes());
    packet.extend_from_slice(body.as_bytes());
    packet.extend_from_slice(&[0, 0]);
    stream.write_all(&packet).await
}

#[tokio::test]
async fn community_server_stores_report_token() {
    with_test_app(async |db, _config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, 'Token社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players)
                   VALUES ($1, $2, 'Token服', '127.0.0.1', 27015, 'secret', 'plugin-token', 'online', $3)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let groups = crate::services::community_service::list_groups(&db).await?;
            assert_eq!(groups[0].servers[0].report_token.as_deref(), Some("plugin-token"));
            Ok(())
        }).await;
}

#[tokio::test]
async fn community_servers_include_access_control_fields() {
    with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '准入社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled
                   )
                   VALUES ($1, $2, '限制服', '127.0.0.1', 27015, 'secret', 'access-token', 'online', $3, true, 1200, 10, true)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = test_app(config, db);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/community/servers")
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let server = &payload["groups"][0]["servers"][0];
            assert_eq!(server["access_restriction_enabled"], true);
            assert_eq!(server["min_rating"], 1200);
            assert_eq!(server["min_steam_level"], 10);
            assert_eq!(server["whitelist_mode_enabled"], true);
            Ok(())
        }).await;
}

#[tokio::test]
async fn admin_can_create_server_with_access_control_config() {
    with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '创建准入社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            let (rcon_port, rcon_server) = spawn_fake_rcon_server().await;
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = test_app(config, db.clone());

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/community/groups/{community_id}/servers"))
                        .header("authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            json!({
                                "name": "准入服",
                                "ip": "127.0.0.1",
                                "port": rcon_port,
                                "rcon_password": "secret",
                                "report_token": "access-token-create",
                                "note": "限制开启",
                                "access_restriction_enabled": true,
                                "min_rating": 1500,
                                "min_steam_level": 12,
                                "whitelist_mode_enabled": true
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            rcon_server.await.unwrap()?;
            assert_eq!(response.status(), StatusCode::CREATED);

            let saved = sqlx::query_as::<_, (bool, i32, i32, bool)>(
                r#"SELECT access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled
                   FROM servers WHERE report_token = $1"#,
            )
            .bind("access-token-create")
            .fetch_one(&db.pool)
            .await?;

            assert_eq!(saved, (true, 1500, 12, true));
            Ok(())
        }).await;
}

#[tokio::test]
async fn community_servers_counts_players_from_report_details() {
    with_test_app(async |db, config| {
        let (_, server_id) = insert_community_with_server(&db, "真实人数统计").await;
        sqlx::query(r#"UPDATE servers SET players = $2 WHERE id = $1"#)
            .bind(server_id)
            .bind(vec!["残留玩家".to_string()])
            .execute(&db.pool)
            .await?;

        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);

        let request = Request::builder()
            .method("GET")
            .uri("/api/community/servers")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let server = &payload["groups"][0]["servers"][0];
        assert_eq!(server["online_player_count"], 0);
        assert_eq!(server["players"].as_array().unwrap().len(), 0);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn normal_admin_cannot_delete_community_group() {
    with_test_app(async |db, config| {
        let (group_id, _) = insert_community_with_server(&db, "普通管理员不可删除").await;
        let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
        let app = test_app(config, db);

        let request = Request::builder()
            .method("DELETE")
            .uri(format!("/api/community/groups/{group_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_can_delete_community_group_and_cascade_servers() {
    with_test_app(async |db, config| {
        let (group_id, server_id) = insert_community_with_server(&db, "系统管理员可删除").await;
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db.clone());

        let request = Request::builder()
            .method("DELETE")
            .uri(format!("/api/community/groups/{group_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let community_count: (i64,) =
            sqlx::query_as(r#"SELECT COUNT(*) FROM communities WHERE id = $1"#)
                .bind(group_id)
                .fetch_one(&db.pool)
                .await?;
        let server_count: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM servers WHERE id = $1"#)
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;

        assert_eq!(community_count.0, 0);
        assert_eq!(server_count.0, 0);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_can_view_and_reset_server_report_token() {
    with_test_app(async |db, config| {
        let (_, server_id) = insert_community_with_server(&db, "Token 管理").await;
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db.clone());

        let request = Request::builder()
            .method("GET")
            .uri(format!("/api/community/servers/{server_id}/report-token"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["token"]["report_token"], "plugin-token");

        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/community/servers/{server_id}/report-token/reset"
            ))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_ne!(payload["token"]["report_token"], "plugin-token");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn normal_admin_cannot_view_server_report_token() {
    with_test_app(async |db, config| {
        let (_, server_id) = insert_community_with_server(&db, "Token 权限").await;
        let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
        let app = test_app(config, db);

        let request = Request::builder()
            .method("GET")
            .uri(format!("/api/community/servers/{server_id}/report-token"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn access_check_rejects_banned_player_before_other_rules() {
    with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            let server_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '封禁优先社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled
                   )
                   VALUES ($1, $2, '封禁优先服', '127.0.0.1', 27015, 'secret', 'access-token-ban', 'online', $3, false, 0, 0, false)"#,
            )
            .bind(server_id)
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"INSERT INTO ban_records (id, player, steam_id, ban_type, reason, duration_minutes, status, operator_name, source)
                   VALUES ($1, 'bad-player', '76561198000000001', 'steam', '作弊', 0, 'active', 'ConsoleAdmin', 'manual')"#,
            )
            .bind(Uuid::new_v4())
            .execute(&db.pool)
            .await?;

            let app = test_app(config, db);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-ban","port":27015,"steam_id64":"76561198000000001"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "你已被永久封禁，原因：作弊");
            Ok(())
        }).await;
}

#[tokio::test]
async fn access_check_allows_when_no_access_modes_enabled() {
    with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '自由社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players)
                   VALUES ($1, $2, '自由服', '127.0.0.1', 27015, 'secret', 'access-token-open', 'online', $3)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let app = test_app(config, db);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-open","port":27015,"steam_id64":"76561198000000002"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], true);
            assert_eq!(payload["result"]["message"], "允许进入服务器。");
            Ok(())
        }).await;
}

#[tokio::test]
async fn access_check_whitelist_mode_requires_approved_global_whitelist() {
    with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '白名单社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players, whitelist_mode_enabled, use_custom_access)
                   VALUES ($1, $2, '白名单服', '127.0.0.1', 27015, 'secret', 'access-token-whitelist', 'online', $3, true, true)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;
            let app = test_app(config, db.clone());

            let rejected = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-whitelist","port":27015,"steam_id64":"76561198000000003"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(rejected.status(), StatusCode::OK);
            let body = to_bytes(rejected.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "你尚未通过白名单审核，无法进入服务器。");

            insert_whitelist_for_steamid64(&db, "76561198000000003", "approved").await?;
            let approved = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-whitelist","port":27015,"steam_id64":"76561198000000003"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(approved.status(), StatusCode::OK);
            let body = to_bytes(approved.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], true);
            assert_eq!(payload["result"]["message"], "已通过白名单审核，允许进入服务器。");
            Ok(())
        }).await;
}

#[tokio::test]
async fn access_check_restriction_uses_success_cache_and_rejects_low_values() {
    with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '限制社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled, use_custom_access
                   )
                   VALUES ($1, $2, '限制服', '127.0.0.1', 27015, 'secret', 'access-token-restrict', 'online', $3, true, 2000, 20, false, true)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;
            sqlx::query(
                r#"INSERT INTO player_access_cache (steamid64, rating, steam_level, expires_at)
                   VALUES ($1, 1999, 30, now() + interval '24 hours')"#,
            )
            .bind("76561198000000004")
            .execute(&db.pool)
            .await?;

            let app = test_app(config, db);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-restrict","port":27015,"steam_id64":"76561198000000004"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "你的 GOKZ rating 未达到服务器最低要求。");
            Ok(())
        }).await;
}

async fn insert_whitelist_for_steamid64(
    db: &Database,
    steamid64: &str,
    status: &str,
) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
            r#"INSERT INTO whitelist_requests (
                id, nickname, steam_id, steamid64, steamid, steamid3, profile_url, status, applied_at, updated_at
               )
               VALUES ($1, $2, $3, $3, $4, $5, $6, $7, now(), now())"#,
        )
        .bind(id)
        .bind(format!("玩家{steamid64}"))
        .bind(steamid64)
        .bind("STEAM_0:1:1")
        .bind("[U:1:1]")
        .bind(format!("https://steamcommunity.com/profiles/{steamid64}"))
        .bind(status)
        .execute(&db.pool)
        .await?;
    Ok(id)
}

#[tokio::test]
async fn access_check_rejects_when_profile_lookup_fails_and_does_not_cache_failure() {
    with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '查询失败社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level, use_custom_access
                   )
                   VALUES ($1, $2, '查询失败拒绝服', '127.0.0.1', 27015, 'secret', 'access-token-fail-closed', 'online', $3, true, 9999, 99, true)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let app = test_app(config, db.clone());
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-fail-closed","port":27015,"steam_id64":"76561198000000005"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "无法验证您的进入资格，请稍后再试。");

            let cache_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM player_access_cache WHERE steamid64 = $1")
                .bind("76561198000000005")
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(cache_count.0, 0);
            Ok(())
        }).await;
}

#[tokio::test]
async fn admin_can_list_real_player_api_rows_and_configure_distribution_limit() {
    with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let (_, server_id) = insert_community_with_server(&db, "玩家信息 API 服").await;
            sqlx::query(
                r#"INSERT INTO server_online_players (server_id, name, steam_id64, ip, ping, server_port, reported_at)
                   VALUES ($1, 'Alice', '76561198000000001', '203.0.113.10', 28, 25575, now())"#,
            )
            .bind(server_id)
            .execute(&db.pool)
            .await?;

            let app = test_app(config.clone(), db.clone());
            let players_request = Request::builder()
                .method("GET")
                .uri("/api/player-api/players")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let players_response = app.oneshot(players_request).await.unwrap();
            assert_eq!(players_response.status(), StatusCode::OK);
            let bytes = to_bytes(players_response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["items"][0]["player"], "Alice");
            assert_eq!(payload["items"][0]["steam_id64"], "76561198000000001");
            assert_eq!(payload["items"][0]["server_name"], "一号服");
            assert_eq!(payload["items"][0]["server_port"], 25575);

            let app = test_app(config.clone(), db.clone());
            let config_request = Request::builder()
                .method("PUT")
                .uri("/api/player-api/config")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "max_api_count": 1,
                    "interval_seconds": 45,
                    "items": [
                        {"public_path":"my-hook","webhook_url":"https://api.example.com/a","secret":null,"server_ids":[server_id]}
                    ]
                }).to_string()))
                .unwrap();
            let config_response = app.oneshot(config_request).await.unwrap();
            assert_eq!(config_response.status(), StatusCode::OK);
            let bytes = to_bytes(config_response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["config"]["max_api_count"], 1);
            assert_eq!(payload["config"]["interval_seconds"], 45);
            assert_eq!(payload["config"]["items"][0]["public_path"], "my-hook");
            assert_eq!(payload["config"]["items"][0]["webhook_url"], "https://api.example.com/a");

            let app = test_app(config, db);
            let over_limit_request = Request::builder()
                .method("PUT")
                .uri("/api/player-api/config")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "max_api_count": 1,
                    "interval_seconds": 45,
                    "items": [
                        {"webhook_url":"https://api.example.com/a","secret":null,"server_ids":[]},
                        {"webhook_url":"https://api.example.com/b","secret":null,"server_ids":[]}
                    ]
                }).to_string()))
                .unwrap();
            let over_limit_response = app.oneshot(over_limit_request).await.unwrap();
            assert_eq!(over_limit_response.status(), StatusCode::BAD_REQUEST);
            Ok(())
        }).await;
}

#[tokio::test]
async fn player_api_dispatch_posts_webhook_and_records_status() {
    with_test_app(async |db, _config| {
            let (_, server_id) = insert_community_with_server(&db, "Webhook 分发服").await;
            sqlx::query(
                r#"INSERT INTO server_online_players (server_id, name, steam_id64, ip, ping, server_port, reported_at)
                   VALUES ($1, 'Alice', '76561198000000001', '203.0.113.10', 28, 25575, now())"#,
            )
            .bind(server_id)
            .execute(&db.pool)
            .await?;

            let (url, server) = spawn_webhook_server("200 OK").await;
            let saved = crate::services::player_api_service::save_config(
                &db,
                crate::services::player_api_service::PlayerApiConfigInput {
                    max_api_count: 1,
                    interval_seconds: 30,
                    items: vec![crate::services::player_api_service::PlayerApiWebhookInput {
                        public_path: "test-webhook".to_string(),
                        webhook_url: Some(url),
                        secret: Some("dispatch-secret".to_string()),
                        server_ids: vec![server_id],
                        external_server_ids: vec![],
                        enabled: true,
                        public_access: true,
                    }],
                },
            )
            .await?;

            crate::services::player_api_service::dispatch_once(&db, &reqwest::Client::new()).await?;
            let request = server.await.unwrap()?;
            assert!(request.contains("POST /"));
            assert!(request.to_ascii_lowercase().contains("x-manger-secret: dispatch-secret"));

            let row: (Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT last_status, last_error, last_dispatched_at FROM player_api_webhooks WHERE id = $1"#,
            )
            .bind(saved.items[0].id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(row.0.as_deref(), Some("success"));
            assert!(row.1.is_none());
            assert!(row.2.is_some());
            Ok(())
        }).await;
}

#[tokio::test]
async fn player_api_dispatch_records_failed_webhook_status() {
    with_test_app(async |db, _config| {
            let (_, server_id) = insert_community_with_server(&db, "Webhook 失败服").await;
            let (url, server) = spawn_webhook_server("500 Internal Server Error").await;
            let saved = crate::services::player_api_service::save_config(
                &db,
                crate::services::player_api_service::PlayerApiConfigInput {
                    max_api_count: 1,
                    interval_seconds: 30,
                    items: vec![crate::services::player_api_service::PlayerApiWebhookInput {
                        public_path: "test-webhook".to_string(),
                        webhook_url: Some(url),
                        secret: None,
                        server_ids: vec![server_id],
                        external_server_ids: vec![],
                        enabled: true,
                        public_access: true,
                    }],
                },
            )
            .await?;

            crate::services::player_api_service::dispatch_once(&db, &reqwest::Client::new()).await?;
            let _ = server.await.unwrap()?;

            let row: (Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT last_status, last_error, last_dispatched_at FROM player_api_webhooks WHERE id = $1"#,
            )
            .bind(saved.items[0].id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(row.0.as_deref(), Some("failed"));
            assert_eq!(row.1.as_deref(), Some("HTTP 500 Internal Server Error"));
            assert!(row.2.is_some());
            Ok(())
        }).await;
}

#[tokio::test]
async fn normal_admin_cannot_manage_player_api_config() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("PUT")
            .uri("/api/player-api/config")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"max_api_count":1,"interval_seconds":30,"items":[]} ).to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn plugin_report_updates_online_players_when_token_and_port_match() {
    with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "插件上报").await;
            let app = test_app(config, db.clone());

            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/online-players/report")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "players": [
                        {
                            "name": "Alice",
                            "steam_id64": "76561198000000001",
                            "ip": "203.0.113.10",
                            "ping": 28,
                            "server_port": 25575
                        }
                    ]
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let server: (String, Vec<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT status, players, last_reported_at FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(server.0, "online");
            assert_eq!(server.1, vec!["Alice".to_string()]);
            assert!(server.2.is_some());

            let player: (String, String, String, i32, i32) = sqlx::query_as(
                r#"SELECT name, steam_id64, ip, ping, server_port FROM server_online_players WHERE server_id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(player.0, "Alice");
            assert_eq!(player.1, "76561198000000001");
            assert_eq!(player.2, "203.0.113.10");
            assert_eq!(player.3, 28);
            assert_eq!(player.4, 25575);
            Ok(())
        }).await;
}

#[tokio::test]
async fn plugin_report_accepts_legacy_payload_with_steam2_id() {
    with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "旧插件上报").await;
            let app = test_app(config, db.clone());

            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/online-players/report")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "players": [
                        {
                            "name": "LegacyAlice",
                            "steam_id": "STEAM_0:1:1",
                            "team": "2",
                            "score": 15,
                            "ping": 42,
                            "connected_seconds": 120
                        }
                    ]
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let server: (String, Vec<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT status, players, last_reported_at FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(server.0, "online");
            assert_eq!(server.1, vec!["LegacyAlice".to_string()]);
            assert!(server.2.is_some());

            let player: (String, String, String, i32, i32) = sqlx::query_as(
                r#"SELECT name, steam_id64, ip, ping, server_port FROM server_online_players WHERE server_id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(player.0, "LegacyAlice");
            assert_eq!(player.1, "76561197960265731");
            assert_eq!(player.2, "unknown");
            assert_eq!(player.3, 42);
            assert_eq!(player.4, 25575);
            Ok(())
        }).await;
}

#[tokio::test]
async fn plugin_report_rejects_matching_token_with_wrong_port() {
    with_test_app(async |db, config| {
        let _ = insert_community_with_server(&db, "插件上报拒绝").await;
        let app = test_app(config, db);

        let request = Request::builder()
            .method("POST")
            .uri("/api/plugin/online-players/report")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "report_token": "plugin-token",
                    "port": 27016,
                    "players": []
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn submit_whitelist_returns_json_body() {
    with_test_app(async |db, config| {
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri("/api/public/whitelist")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "steam_input": "76561197960290419",
                    "nickname": "测试玩家"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["item"]["status"], "pending");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_can_list_api_endpoint_docs() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("GET")
            .uri("/api/docs/endpoints")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let items = payload["items"].as_array().unwrap();
        assert!(items
            .iter()
            .any(|item| item["endpoint"] == "/api/docs/endpoints"));
        assert!(items
            .iter()
            .any(|item| item["endpoint"] == "/api/bans" && item["method"] == "POST"));
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn normal_admin_cannot_list_api_endpoint_docs() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("GET")
            .uri("/api/docs/endpoints")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn approve_whitelist_with_authenticated_operator() {
    with_test_app(async |db, config| {
        let id = insert_whitelist(&db, "pending").await;
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/whitelist/{id}/approve"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["item"]["status"], "approved");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn restore_whitelist_with_authenticated_operator() {
    with_test_app(async |db, config| {
        let id = insert_whitelist(&db, "rejected").await;
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/whitelist/{id}/restore"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["item"]["status"], "approved");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_can_create_manual_ip_ban_with_missing_player_and_ip() {
    with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = test_app(config, db.clone());
            let request = Request::builder()
                .method("POST")
                .uri("/api/bans")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "player": null,
                    "steam_id": "76561198000000000",
                    "ban_type": "ip",
                    "ip_address": null,
                    "reason": "重复违规"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);

            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(payload["item"]["player"].is_null());
            assert_eq!(payload["item"]["steam_id"], "76561198000000000");
            assert_eq!(payload["item"]["ban_type"], "ip");
            assert!(payload["item"]["ip_address"].is_null());
            assert_eq!(payload["item"]["reason"], "重复违规");
            assert_eq!(payload["item"]["duration_minutes"], 0);
            assert!(payload["item"]["expires_at"].is_null());
            assert_eq!(payload["item"]["source"], "manual");
            assert!(payload["item"]["server_id"].is_null());
            assert!(payload["item"]["server_port"].is_null());

            let saved = sqlx::query_as::<_, (Option<String>, String, String, Option<String>, String, i32, Option<chrono::DateTime<chrono::Utc>>, String)>(
                r#"SELECT player, steam_id, ban_type, ip_address, reason, duration_minutes, expires_at, source FROM ban_records WHERE steam_id = $1"#,
            )
            .bind("76561198000000000")
            .fetch_one(&db.pool)
            .await?;

            assert_eq!(saved.0, None);
            assert_eq!(saved.1, "76561198000000000");
            assert_eq!(saved.2, "ip");
            assert_eq!(saved.3, None);
            assert_eq!(saved.4, "重复违规");
            assert_eq!(saved.5, 0);
            assert!(saved.6.is_none());
            assert_eq!(saved.7, "manual");
            Ok(())
        }).await;
}

#[tokio::test]
async fn plugin_ban_check_completes_missing_manual_ban_details() {
    with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "补齐封禁信息服").await;
            let ban_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                       id, player, steam_id, ip_address, server_name, ban_type,
                       duration_minutes, expires_at, reason, status, operator_name, source,
                       server_id, server_port
                   )
                   VALUES ($1, NULL, '76561198000000000', NULL, NULL, 'steam',
                           0, NULL, '重复违规', 'active', 'Alex', 'manual', NULL, NULL)"#,
            )
            .bind(ban_id)
            .execute(&db.pool)
            .await?;

            let app = test_app(config, db.clone());
            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans/check")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "steam_id": "76561198000000000",
                    "ip_address": "192.168.1.55",
                    "player": "LatePlayer",
                    "server_port": 25575
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let saved = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>, Option<Uuid>, Option<i32>)>(
                r#"SELECT player, ip_address, server_name, server_id, server_port FROM ban_records WHERE id = $1"#,
            )
            .bind(ban_id)
            .fetch_one(&db.pool)
            .await?;

            assert_eq!(saved.0.as_deref(), Some("LatePlayer"));
            assert_eq!(saved.1.as_deref(), Some("192.168.1.55"));
            assert_eq!(saved.2.as_deref(), Some("一号服"));
            assert_eq!(saved.3, Some(server_id));
            assert_eq!(saved.4, Some(25575));
            Ok(())
        }).await;
}

#[tokio::test]
async fn plugin_can_create_timed_ban() {
    with_test_app(async |db, config| {
        let (_, _) = insert_community_with_server(&db, "插件封禁服").await;
        let app = test_app(config, db.clone());
        let request = Request::builder()
            .method("POST")
            .uri("/api/plugin/bans")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "ban_type": "steam",
                    "steam_id": "STEAM_1:1:12345",
                    "ip_address": "192.168.1.20",
                    "player": "BadPlayer",
                    "duration_minutes": 30,
                    "reason": "作弊",
                    "operator_name": "ConsoleAdmin"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["item"]["source"], "game_plugin");
        assert_eq!(payload["item"]["duration_minutes"], 30);
        assert!(payload["item"]["expires_at"].is_string());
        assert_eq!(
            payload["kick_message"],
            "你已被封禁，原因：作弊，到期时间：".to_string()
                + payload["item"]["expires_at"].as_str().unwrap()
        );
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn plugin_ban_rejects_invalid_token() {
    with_test_app(async |db, config| {
        let (_, _) = insert_community_with_server(&db, "插件封禁服").await;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri("/api/plugin/bans")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "report_token": "wrong-token",
                    "port": 25575,
                    "ban_type": "steam",
                    "steam_id": "STEAM_1:1:12345",
                    "ip_address": "192.168.1.20",
                    "player": "BadPlayer",
                    "duration_minutes": 0,
                    "reason": "作弊",
                    "operator_name": "ConsoleAdmin"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn plugin_can_check_and_unban_target() {
    with_test_app(async |db, config| {
        let (_, _) = insert_community_with_server(&db, "插件封禁服").await;
        let app = test_app(config.clone(), db.clone());
        let create_request = Request::builder()
            .method("POST")
            .uri("/api/plugin/bans")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "ban_type": "ip",
                    "steam_id": null,
                    "ip_address": "192.168.1.21",
                    "player": "IpPlayer",
                    "duration_minutes": 0,
                    "reason": "恶意行为",
                    "operator_name": "ConsoleAdmin"
                })
                .to_string(),
            ))
            .unwrap();
        let create_response = app.oneshot(create_request).await.unwrap();
        assert_eq!(create_response.status(), StatusCode::CREATED);

        let app = test_app(config.clone(), db.clone());
        let check_request = Request::builder()
            .method("POST")
            .uri("/api/plugin/bans/check")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "steam_id": "STEAM_1:1:999",
                    "ip_address": "192.168.1.21"
                })
                .to_string(),
            ))
            .unwrap();
        let check_response = app.oneshot(check_request).await.unwrap();
        assert_eq!(check_response.status(), StatusCode::OK);
        let bytes = to_bytes(check_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["banned"], true);

        let app = test_app(config, db);
        let unban_request = Request::builder()
            .method("POST")
            .uri("/api/plugin/bans/unban")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "target": "192.168.1.21",
                    "reason": "申诉通过",
                    "operator_name": "ConsoleAdmin"
                })
                .to_string(),
            ))
            .unwrap();
        let unban_response = app.oneshot(unban_request).await.unwrap();
        assert_eq!(unban_response.status(), StatusCode::OK);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn plugin_can_poll_active_bans_for_server() {
    with_test_app(async |db, config| {
        let (_, server_id) = insert_community_with_server(&db, "插件轮询封禁服").await;
        sqlx::query(
            r#"INSERT INTO ban_records (
                       id, player, steam_id, ip_address, server_name, ban_type,
                       duration_minutes, expires_at, reason, status, operator_name, source,
                       server_id, server_port
                   )
                   VALUES ($1, 'BadPlayer', '76561197960290419', '192.168.1.30', '一号服', 'steam',
                           0, NULL, '作弊', 'active', 'Alex', 'manual', $2, 25575),
                          ($3, 'OldPlayer', '76561197960290421', '192.168.1.31', '一号服', 'steam',
                           0, NULL, '已解封', 'inactive', 'Alex', 'manual', $2, 25575)"#,
        )
        .bind(Uuid::new_v4())
        .bind(server_id)
        .bind(Uuid::new_v4())
        .execute(&db.pool)
        .await?;

        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri("/api/plugin/bans/poll")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "report_token": "plugin-token",
                    "port": 25575
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["items"].as_array().unwrap().len(), 1);
        assert_eq!(payload["items"][0]["steam_id"], "76561197960290419");
        assert_eq!(payload["items"][0]["ip_address"], "192.168.1.30");
        assert_eq!(payload["items"][0]["reason"], "作弊");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn create_manual_ban_rejects_missing_reason() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri("/api/bans")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "player": "Alex",
                    "steam_id": "76561198000000001",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.5",
                    "reason": "   "
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["error"], "封禁理由不能为空");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_can_update_and_delete_manual_ban() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let ban_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO ban_records (
                       id, player, steam_id, ip_address, server_name, ban_type,
                       duration_minutes, expires_at, reason, status, operator_name, source,
                       server_id, server_port
                   )
                   VALUES ($1, 'OldName', '76561197960290419', '192.168.1.5', NULL, 'steam',
                           0, NULL, '旧原因', 'active', 'Alex', 'manual', NULL, NULL)"#,
        )
        .bind(ban_id)
        .execute(&db.pool)
        .await?;

        let app = test_app(config.clone(), db.clone());
        let update_request = Request::builder()
            .method("PUT")
            .uri(format!("/api/bans/{ban_id}"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "player": "NewName",
                    "steam_id": "STEAM_0:1:12345",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.6",
                    "reason": "新原因"
                })
                .to_string(),
            ))
            .unwrap();
        let update_response = app.oneshot(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);
        let bytes = to_bytes(update_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["item"]["player"], "NewName");
        assert_eq!(payload["item"]["steam_id"], "76561197960290419");
        assert_eq!(payload["item"]["ip_address"], "192.168.1.6");
        assert_eq!(payload["item"]["reason"], "新原因");

        let app = test_app(config, db.clone());
        let delete_request = Request::builder()
            .method("DELETE")
            .uri(format!("/api/bans/{ban_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let delete_response = app.oneshot(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

        let count: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM ban_records WHERE id = $1"#)
            .bind(ban_id)
            .fetch_one(&db.pool)
            .await?;
        assert_eq!(count.0, 0);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn create_manual_ban_normalizes_steamid2_to_steamid64() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db.clone());
        let request = Request::builder()
            .method("POST")
            .uri("/api/bans")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "player": "Alex",
                    "steam_id": "STEAM_0:1:12345",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.5",
                    "reason": "作弊"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["item"]["steam_id"], "76561197960290419");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn create_manual_ban_rejects_missing_steamid64() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri("/api/bans")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "player": "Alex",
                    "steam_id": "   ",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.5",
                    "reason": "作弊"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["error"], "SteamID64 不能为空");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn create_manual_ban_rejects_invalid_ban_type() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri("/api/bans")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "player": "Alex",
                    "steam_id": "76561198000000002",
                    "ban_type": "hardware",
                    "ip_address": "192.168.1.6",
                    "reason": "作弊"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["error"], "封禁属性无效");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn normal_admin_only_sees_self_in_users_list() {
    with_test_app(async |db, config| {
        let app = test_app(config, db.clone());
        let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;

        let request = Request::builder()
            .method("GET")
            .uri("/api/users")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["items"].as_array().unwrap().len(), 1);
        assert_eq!(payload["items"][0]["role"], "normal");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_cannot_update_developer_user() {
    with_test_app(async |db, config| {
        let app = test_app(config, db.clone());
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;

        let request = Request::builder()
            .method("PUT")
            .uri("/api/users/22222222-2222-2222-2222-222222222222")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "username": "devadmin2",
                    "role": "developer",
                    "steam_id": "76561198000000000",
                    "remark": "should fail"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn normal_admin_can_approve_but_cannot_revoke_whitelist() {
    with_test_app(async |db, config| {
        let pending_id = insert_whitelist(&db, "pending").await;
        let approved_id = insert_whitelist(&db, "approved").await;
        let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
        let app = test_app(config, db);

        let approve_request = Request::builder()
            .method("POST")
            .uri(format!("/api/whitelist/{pending_id}/approve"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let approve_response = app.clone().oneshot(approve_request).await.unwrap();
        assert_eq!(approve_response.status(), StatusCode::OK);

        let revoke_request = Request::builder()
            .method("POST")
            .uri(format!("/api/whitelist/{approved_id}/revoke"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let revoke_response = app.oneshot(revoke_request).await.unwrap();
        assert_eq!(revoke_response.status(), StatusCode::FORBIDDEN);
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_can_create_user_without_steam_id() {
    with_test_app(async |db, config| {
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("POST")
            .uri("/api/users")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "username": "new_admin_without_steam",
                    "password": "secret123",
                    "role": "normal",
                    "steam_id": null,
                    "remark": "无 steamid"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(payload["item"]["steam_id"].is_null());
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn admin_can_clear_user_steam_id() {
    with_test_app(async |db, config| {
        ensure_test_user_exists(&db, "33333333-3333-3333-3333-333333333333").await?;
        let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
        let app = test_app(config, db);
        let request = Request::builder()
            .method("PUT")
            .uri("/api/users/33333333-3333-3333-3333-333333333333")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "username": "james",
                    "role": "normal",
                    "steam_id": null,
                    "remark": "清空 steamid"
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(payload["item"]["steam_id"].is_null());
        Ok(())
    })
    .await;
}
