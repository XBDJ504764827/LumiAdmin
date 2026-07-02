use super::Database;
use crate::{
    config::Config,
    services::{dashboard_service, public_service, whitelist_service},
};

use uuid::Uuid;

fn schema_url(base_url: &str, schema: &str) -> String {
    crate::test_util::schema_url(base_url, schema)
}

async fn create_schema(base_url: &str, schema: &str) {
    crate::test_util::create_schema(base_url, schema).await;
}

async fn drop_schema(base_url: &str, schema: &str) {
    crate::test_util::drop_schema(base_url, schema).await;
}

#[tokio::test]
async fn dashboard_metrics_count_online_players_from_player_array() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
                .bind(community_id)
                .bind("像素方块社区")
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, players)
                   VALUES ($1, $2, $3, $4, $5, $6, 'online', $7),
                          ($8, $2, $9, $10, $11, $12, 'offline', $13)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind("在线服")
            .bind("127.0.0.1")
            .bind(25575_i32)
            .bind("secret")
            .bind(vec!["玩家甲".to_string(), "玩家乙".to_string(), "玩家丙".to_string()])
            .bind(Uuid::new_v4())
            .bind("离线服")
            .bind("127.0.0.2")
            .bind(25576_i32)
            .bind("secret")
            .bind(vec!["不应统计".to_string()])
            .execute(&db.pool)
            .await?;

            let metrics = dashboard_service::get_metrics(&db).await?;
            assert_eq!(metrics.online_players, 3);

            Ok::<(), anyhow::Error>(())
        }
        .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn dashboard_metrics_include_all_admin_roles_in_preview() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;
        db.migrate().await?;

        sqlx::query(
            r#"INSERT INTO users (id, username, display_name, password_hash, role, remark)
                   VALUES ($1, 'admin_preview_admin', 'Admin One', 'pw', 'admin', 'Admin Remark'),
                          ($2, 'admin_preview_dev', 'Dev One', 'pw', 'developer', NULL),
                          ($3, 'admin_preview_normal', 'Normal One', 'pw', 'normal', ''),
                          ($4, 'admin_preview_guest', 'Guest One', 'pw', 'guest', 'Guest Remark')"#,
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .execute(&db.pool)
        .await?;

        let metrics = dashboard_service::get_metrics(&db).await?;
        let mut preview_names: Vec<_> = metrics
            .admin_preview
            .iter()
            .map(|item| item.display_name.as_str())
            .collect();
        preview_names.sort_unstable();

        assert_eq!(metrics.admins, 3);
        assert_eq!(
            preview_names,
            vec!["Admin Remark", "admin_preview_dev", "admin_preview_normal"]
        );
        assert!(metrics
            .admin_preview
            .iter()
            .all(|item| item.status == "可用"));

        Ok::<(), anyhow::Error>(())
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_converts_legacy_players_text_to_text_array() {
    let config = Config::from_env();
    let base_url = config.database_url;
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE communities (
                  id UUID PRIMARY KEY,
                  name TEXT NOT NULL,
                  created_by UUID,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"CREATE TABLE servers (
                  id UUID PRIMARY KEY,
                  community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
                  name TEXT NOT NULL,
                  ip TEXT,
                  port INTEGER,
                  rcon_password TEXT,
                  note TEXT,
                  status TEXT NOT NULL DEFAULT 'untested',
                  players TEXT NOT NULL DEFAULT '',
                  last_tested_at TIMESTAMPTZ,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            let community_id = Uuid::new_v4();
            let server_id = Uuid::new_v4();

            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
                .bind(community_id)
                .bind("像素方块社区")
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, players)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            )
            .bind(server_id)
            .bind(community_id)
            .bind("一号服")
            .bind("127.0.0.1")
            .bind(25575_i32)
            .bind("secret")
            .bind("online")
            .bind("{测试玩家A,测试玩家B}")
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let players_udt_name: (String,) = sqlx::query_as(
                r#"SELECT udt_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'servers'
                     AND column_name = 'players'"#,
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(players_udt_name.0, "_text");

            let stored_players: (Vec<String>,) = sqlx::query_as(
                r#"SELECT players FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(stored_players.0, vec!["测试玩家A", "测试玩家B"]);

            Ok::<(), anyhow::Error>(())
        }
        .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_adds_server_report_tokens_and_online_players_table() {
    let config = Config::from_env();
    let base_url = config.database_url;
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE communities (
                  id UUID PRIMARY KEY,
                  name TEXT NOT NULL,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"CREATE TABLE servers (
                  id UUID PRIMARY KEY,
                  community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
                  name TEXT NOT NULL,
                  ip TEXT NOT NULL,
                  port INTEGER NOT NULL,
                  rcon_password TEXT NOT NULL,
                  status TEXT NOT NULL DEFAULT 'untested',
                  players TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
            )
            .execute(&db.pool)
            .await?;

            let community_id = Uuid::new_v4();
            let server_id = Uuid::new_v4();

            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
                .bind(community_id)
                .bind("Token 迁移社区")
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, status, players)
                   VALUES ($1, $2, $3, $4, $5, $6, 'online', $7)"#,
            )
            .bind(server_id)
            .bind(community_id)
            .bind("一号服")
            .bind("127.0.0.1")
            .bind(27015_i32)
            .bind("secret")
            .bind(vec!["测试玩家".to_string()])
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let token_and_reported_at: (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT report_token, last_reported_at FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert!(!token_and_reported_at.0.is_empty());
            assert!(token_and_reported_at.1.is_none());

            let table_count: (i64,) = sqlx::query_as(
                r#"SELECT COUNT(*)
                   FROM information_schema.tables
                   WHERE table_schema = current_schema()
                     AND table_name = 'server_online_players'"#,
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(table_count.0, 1);

            Ok::<(), anyhow::Error>(())
        }
        .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_creates_player_api_distribution_config() {
    let config = Config::from_env();
    let base_url = config.database_url;
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;
        db.migrate().await?;

        let config_row: (i32, i32) = sqlx::query_as(
            r#"SELECT max_api_count, interval_seconds FROM player_api_config WHERE id = true"#,
        )
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(config_row, (3, 30));

        let webhook_columns: Vec<(String,)> = sqlx::query_as(
            r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'player_api_webhooks'
                   ORDER BY column_name"#,
        )
        .fetch_all(&db.pool)
        .await?;
        let names = webhook_columns
            .into_iter()
            .map(|row| row.0)
            .collect::<Vec<_>>();
        assert!(names.contains(&"webhook_url".to_string()));
        assert!(names.contains(&"secret".to_string()));
        assert!(names.contains(&"server_ids".to_string()));
        assert!(names.contains(&"last_status".to_string()));
        assert!(names.contains(&"last_error".to_string()));
        assert!(names.contains(&"last_dispatched_at".to_string()));

        Ok::<(), anyhow::Error>(())
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_expands_whitelist_requests_schema() {
    let config = Config::from_env();
    let base_url = config.database_url;
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;
        db.migrate().await?;

        let columns: Vec<(String,)> = sqlx::query_as(
            r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'whitelist_requests'
                   ORDER BY column_name"#,
        )
        .fetch_all(&db.pool)
        .await?;

        let names = columns.into_iter().map(|x| x.0).collect::<Vec<_>>();
        assert!(names.contains(&"steamid64".to_string()));
        assert!(names.contains(&"steamid".to_string()));
        assert!(names.contains(&"steamid3".to_string()));
        assert!(names.contains(&"rejection_reason".to_string()));
        assert!(names.contains(&"revoked_at".to_string()));
        assert!(names.contains(&"approved_by".to_string()));
        assert!(names.contains(&"contact".to_string()));

        Ok::<(), anyhow::Error>(())
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_supports_legacy_whitelist_requests_without_created_at() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;

        sqlx::query(
            r#"CREATE TABLE whitelist_requests (
                  id UUID PRIMARY KEY,
                  player_name TEXT NOT NULL,
                  steam_id64 TEXT NOT NULL,
                  steam_id TEXT,
                  steam_profile_url TEXT,
                  source TEXT,
                  status TEXT NOT NULL,
                  reject_reason TEXT,
                  applied_at TIMESTAMPTZ,
                  reviewed_at TIMESTAMPTZ,
                  reviewed_by TEXT
                )"#,
        )
        .execute(&db.pool)
        .await?;

        let rejected_id = Uuid::new_v4();
        let approved_id = Uuid::new_v4();
        let applied_at = chrono::Utc::now() - chrono::Duration::days(1);
        let reviewed_at = chrono::Utc::now();

        sqlx::query(
            r#"INSERT INTO whitelist_requests (
                  id, player_name, steam_id64, steam_id, steam_profile_url, source,
                  status, reject_reason, applied_at, reviewed_at, reviewed_by
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        )
        .bind(rejected_id)
        .bind("旧版被拒玩家")
        .bind("76561198000000001")
        .bind("STEAM_0:1:1")
        .bind("https://steamcommunity.com/profiles/76561198000000001")
        .bind("public")
        .bind("rejected")
        .bind("资料不完整")
        .bind(applied_at)
        .bind(reviewed_at)
        .bind("Alex")
        .execute(&db.pool)
        .await?;

        sqlx::query(
            r#"INSERT INTO whitelist_requests (
                  id, player_name, steam_id64, steam_id, steam_profile_url, source,
                  status, reject_reason, applied_at, reviewed_at, reviewed_by
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8, $9, $10)"#,
        )
        .bind(approved_id)
        .bind("旧版已通过玩家")
        .bind("76561198000000002")
        .bind("STEAM_0:0:2")
        .bind("https://steamcommunity.com/profiles/76561198000000002")
        .bind("manual")
        .bind("approved")
        .bind(applied_at)
        .bind(reviewed_at)
        .bind("DevAdmin")
        .execute(&db.pool)
        .await?;

        db.migrate().await?;

        let items = whitelist_service::list_whitelist(
            &db,
            &crate::routes::ListQuery {
                search: None,
                status: None,
                source: None,
                page: None,
                page_size: None,
            },
        )
        .await?;
        assert_eq!(items.items.len(), 2);
        assert!(items.items.iter().any(|item| {
            item.nickname == "旧版被拒玩家"
                && item.steamid64 == "76561198000000001"
                && item.profile_url.as_deref()
                    == Some("https://steamcommunity.com/profiles/76561198000000001")
                && item.rejection_reason.as_deref() == Some("资料不完整")
                && item.rejected_by.as_deref() == Some("Alex")
                && item.rejected_at.is_some()
        }));
        assert!(items.items.iter().any(|item| {
            item.nickname == "旧版已通过玩家"
                && item.steamid64 == "76561198000000002"
                && item.approved_by.as_deref() == Some("DevAdmin")
                && item.approved_at.is_some()
        }));

        let public_items = public_service::list_public_whitelist(
            &db,
            &crate::routes::ListQuery {
                search: None,
                status: None,
                source: None,
                page: None,
                page_size: None,
            },
        )
        .await?;
        assert_eq!(public_items.items.len(), 1);
        assert_eq!(public_items.items[0].nickname, "旧版已通过玩家");
        assert_eq!(public_items.items[0].steamid64, "76561198000000002");
        assert!(public_items.items[0].approved_at.is_some());

        Ok::<(), anyhow::Error>(())
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_keeps_duplicate_whitelist_requests_steamid64_records() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;

            sqlx::query(
                r#"CREATE TABLE whitelist_requests (
                  id UUID PRIMARY KEY,
                  nickname TEXT NOT NULL,
                  steam_id TEXT,
                  steamid64 TEXT,
                  status TEXT NOT NULL,
                  applied_at TIMESTAMPTZ,
                  updated_at TIMESTAMPTZ
                )"#,
            )
            .execute(&db.pool)
            .await?;

            let duplicate_steamid64 = "76561198000000021";
            let first_id = Uuid::new_v4();
            let second_id = Uuid::new_v4();
            let first_time = chrono::Utc::now() - chrono::Duration::hours(2);
            let second_time = chrono::Utc::now() - chrono::Duration::hours(1);

            sqlx::query(
                r#"INSERT INTO whitelist_requests (id, nickname, steam_id, steamid64, status, applied_at, updated_at)
                   VALUES ($1, $2, $3, $4, 'pending', $5, $5)"#,
            )
            .bind(first_id)
            .bind("旧重复记录")
            .bind("STEAM_0:1:1")
            .bind(duplicate_steamid64)
            .bind(first_time)
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"INSERT INTO whitelist_requests (id, nickname, steam_id, steamid64, status, applied_at, updated_at)
                   VALUES ($1, $2, $3, $4, 'pending', $5, $5)"#,
            )
            .bind(second_id)
            .bind("新重复记录")
            .bind("STEAM_0:1:1")
            .bind(duplicate_steamid64)
            .bind(second_time)
            .execute(&db.pool)
            .await?;

            db.migrate().await?;

            let count: (i64,) = sqlx::query_as(
                r#"SELECT COUNT(*) FROM whitelist_requests WHERE steamid64 = $1"#,
            )
            .bind(duplicate_steamid64)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(count.0, 2);

            Ok::<(), anyhow::Error>(())
        }
        .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_expands_users_schema_for_admin_management() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            db.seed(&config).await?;

            sqlx::query(
                r#"INSERT INTO users (id, username, display_name, password_hash, role)
                   VALUES ('11111111-1111-1111-1111-111111111111', 'test-admin', 'Admin', 'test', 'admin'),
                          ('33333333-3333-3333-3333-333333333333', 'test-normal', 'Normal', 'test', 'normal')
                   ON CONFLICT (id) DO NOTHING"#,
            )
            .execute(&db.pool)
            .await?;

            let columns = sqlx::query_as::<_, (String,)>(
                r#"
                SELECT column_name
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = 'users'
                ORDER BY column_name
                "#,
            )
            .fetch_all(&db.pool)
            .await?;

            let names: Vec<String> = columns.into_iter().map(|row| row.0).collect();
            assert!(names.contains(&"steam_id".to_string()));
            assert!(names.contains(&"remark".to_string()));

            let roles = sqlx::query_as::<_, (String,)>(
                r#"SELECT role FROM users ORDER BY role"#,
            )
            .fetch_all(&db.pool)
            .await?;

            let role_values: Vec<String> = roles.into_iter().map(|row| row.0).collect();
            assert!(role_values.contains(&"admin".to_string()));
            assert!(role_values.contains(&"developer".to_string()));
            assert!(role_values.contains(&"normal".to_string()));

            Ok::<(), anyhow::Error>(())
        }
        .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_expands_ban_records_missing_manual_ban_columns() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;

        sqlx::query(
            r#"CREATE TABLE ban_records (
                  id UUID PRIMARY KEY,
                  steam_id TEXT NOT NULL,
                  status TEXT NOT NULL,
                  operator_name TEXT NOT NULL,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
        )
        .execute(&db.pool)
        .await?;

        db.migrate().await?;

        let columns = sqlx::query_as::<_, (String,)>(
            r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'ban_records'
                   ORDER BY column_name"#,
        )
        .fetch_all(&db.pool)
        .await?;

        let names: Vec<String> = columns.into_iter().map(|row| row.0).collect();
        assert!(names.contains(&"player".to_string()));
        assert!(names.contains(&"ip_address".to_string()));
        assert!(names.contains(&"server_name".to_string()));
        assert!(names.contains(&"ban_type".to_string()));
        assert!(names.contains(&"reason".to_string()));

        Ok::<(), anyhow::Error>(())
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_expands_ban_records_for_manual_ban_creation() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;

        sqlx::query(
            r#"CREATE TABLE ban_records (
                  id UUID PRIMARY KEY,
                  player TEXT NOT NULL,
                  steam_id TEXT NOT NULL,
                  ip_address TEXT NOT NULL,
                  server_name TEXT NOT NULL,
                  status TEXT NOT NULL,
                  operator_name TEXT NOT NULL,
                  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )"#,
        )
        .execute(&db.pool)
        .await?;

        db.migrate().await?;

        let columns = sqlx::query_as::<_, (String, String, String)>(
            r#"
                SELECT column_name, is_nullable, data_type
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = 'ban_records'
                ORDER BY column_name
                "#,
        )
        .fetch_all(&db.pool)
        .await?;

        let column = |name: &str| {
            columns
                .iter()
                .find(|row| row.0 == name)
                .expect("ban_records column exists")
        };

        assert_eq!(column("player").1, "YES");
        assert_eq!(column("ip_address").1, "YES");
        assert_eq!(column("server_name").1, "YES");
        assert_eq!(column("ban_type").1, "NO");
        assert_eq!(column("reason").1, "NO");

        Ok::<(), anyhow::Error>(())
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_adds_plugin_ban_fields() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
        let db = Database::connect_for_test(&scoped_url).await?;
        db.migrate().await?;

        let columns = sqlx::query_as::<_, (String,)>(
            r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'ban_records'"#,
        )
        .fetch_all(&db.pool)
        .await?;
        let names = columns.into_iter().map(|row| row.0).collect::<Vec<_>>();

        assert!(names.contains(&"duration_minutes".to_string()));
        assert!(names.contains(&"expires_at".to_string()));
        assert!(names.contains(&"source".to_string()));
        assert!(names.contains(&"server_id".to_string()));
        assert!(names.contains(&"server_port".to_string()));
        assert!(names.contains(&"removed_reason".to_string()));
        assert!(names.contains(&"removed_by".to_string()));
        assert!(names.contains(&"removed_at".to_string()));

        let server_columns = sqlx::query_as::<_, (String,)>(
            r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'servers'"#,
        )
        .fetch_all(&db.pool)
        .await?;
        let server_names = server_columns
            .into_iter()
            .map(|row| row.0)
            .collect::<Vec<_>>();
        assert!(server_names.contains(&"report_token".to_string()));

        Ok::<(), anyhow::Error>(())
    }
    .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}

#[tokio::test]
async fn migrate_adds_server_access_control_fields_and_cache_table() {
    let config = Config::from_env();
    let base_url = config.database_url.clone();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    let scoped_url = schema_url(&base_url, &schema);

    create_schema(&base_url, &schema).await;

    let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            let server_columns = sqlx::query_scalar::<_, String>(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'servers'
                     AND column_name IN (
                        'access_restriction_enabled',
                        'min_rating',
                        'min_steam_level',
                        'whitelist_mode_enabled'
                     )
                   ORDER BY column_name"#,
            )
            .fetch_all(&db.pool)
            .await?;

            assert_eq!(
                server_columns,
                vec![
                    "access_restriction_enabled".to_string(),
                    "min_rating".to_string(),
                    "min_steam_level".to_string(),
                    "whitelist_mode_enabled".to_string(),
                ]
            );

            let cache_columns = sqlx::query_scalar::<_, String>(
                r#"SELECT column_name
                   FROM information_schema.columns
                   WHERE table_schema = current_schema()
                     AND table_name = 'player_access_cache'
                     AND column_name IN ('steamid64', 'rating', 'steam_level', 'expires_at', 'updated_at')
                   ORDER BY column_name"#,
            )
            .fetch_all(&db.pool)
            .await?;

            assert_eq!(
                cache_columns,
                vec![
                    "expires_at".to_string(),
                    "rating".to_string(),
                    "steam_level".to_string(),
                    "steamid64".to_string(),
                    "updated_at".to_string(),
                ]
            );

            Ok::<(), anyhow::Error>(())
        }
        .await;

    drop_schema(&base_url, &schema).await;
    result.unwrap();
}
