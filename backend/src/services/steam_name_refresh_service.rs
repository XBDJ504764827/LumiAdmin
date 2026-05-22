use crate::config::Config;
use crate::db::Database;
use crate::services::steam_service::SteamResolver;
use futures::stream::{self, StreamExt};
/// 每隔 interval_seconds 秒刷新所有白名单记录的Steam名称
pub fn start_steam_name_refresh_loop(db: Database, config: Config, interval_seconds: u64) {
    tokio::spawn(async move {
        // 首次启动延迟30秒，避免与其他初始化任务冲突
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
        loop {
            interval.tick().await;
            let resolver = SteamResolver::new(&config);
            match refresh_steam_names(&db, &resolver).await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!(count, "定时刷新Steam名称完成");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "定时刷新Steam名称失败");
                }
            }
        }
    });
}

/// 需要刷新的记录
#[derive(sqlx::FromRow)]
struct RefreshRecord {
    id: uuid::Uuid,
    steamid64: String,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
}

/// 刷新所有白名单记录的Steam名称，同时补全缺失的SteamID2/SteamID3/ProfileURL
/// 只刷新 status 为 pending 或 approved 的记录
pub async fn refresh_steam_names(db: &Database, resolver: &SteamResolver) -> anyhow::Result<usize> {
    const CONCURRENT_REQUESTS: usize = 5;
    const BATCH_DELAY_MS: u64 = 500;

    let records: Vec<RefreshRecord> = sqlx::query_as(
        r#"SELECT id, steamid64, steamid, steamid3, profile_url
           FROM whitelist_requests
           WHERE status IN ('pending', 'approved')
             AND (steam_persona_name IS NULL OR btrim(steam_persona_name) = ''
                  OR steamid IS NULL OR btrim(steamid) = ''
                  OR steamid3 IS NULL OR btrim(steamid3) = ''
                  OR profile_url IS NULL OR btrim(profile_url) = '')"#,
    )
    .fetch_all(&db.pool)
    .await?;

    if records.is_empty() {
        return Ok(0);
    }

    let total = records.len();
    tracing::info!(total, "开始刷新Steam资料（名称 + 缺失字段）");

    let mut updated_count = 0;
    let mut processed = 0;

    for chunk in records.chunks(CONCURRENT_REQUESTS) {
        let results: Vec<(uuid::Uuid, Option<RefreshResult>)> = stream::iter(chunk.iter())
            .then(|record| async move {
                let mut result = RefreshResult::default();

                // 获取 Steam Profile（名称）
                if let Some(profile) = resolver.fetch_profile(&record.steamid64).await.ok().flatten() {
                    result.persona_name = Some(profile.persona_name);
                }

                // 本地计算缺失的 SteamID2/SteamID3/ProfileURL
                if let Ok(identity) = resolver.parse_local(&record.steamid64) {
                    if record.steamid.as_deref().map_or(true, |v| v.trim().is_empty()) {
                        result.steamid = identity.steamid;
                    }
                    if record.steamid3.as_deref().map_or(true, |v| v.trim().is_empty()) {
                        result.steamid3 = identity.steamid3;
                    }
                    if record.profile_url.as_deref().map_or(true, |v| v.trim().is_empty()) {
                        result.profile_url = identity.profile_url;
                    }
                }

                let has_update = result.persona_name.is_some()
                    || result.steamid.is_some()
                    || result.steamid3.is_some()
                    || result.profile_url.is_some();

                (record.id, if has_update { Some(result) } else { None })
            })
            .collect()
            .await;

        for (id, result) in results {
            #[allow(unused_assignments)]
            if let Some(r) = result {
                let mut updates = Vec::new();
                let mut param_idx = 2u32;

                if r.persona_name.is_some() {
                    updates.push(format!("steam_persona_name = ${param_idx}"));
                    param_idx += 1;
                }
                if r.steamid.is_some() {
                    updates.push(format!("steamid = ${param_idx}"));
                    param_idx += 1;
                }
                if r.steamid3.is_some() {
                    updates.push(format!("steamid3 = ${param_idx}"));
                    param_idx += 1;
                }
                if r.profile_url.is_some() {
                    updates.push(format!("profile_url = ${param_idx}"));
                    param_idx += 1;
                }

                if updates.is_empty() {
                    processed += 1;
                    continue;
                }

                updates.push("updated_at = now()".to_string());
                let sql = format!(
                    "UPDATE whitelist_requests SET {} WHERE id = $1",
                    updates.join(", ")
                );

                let mut query = sqlx::query(&sql).bind(id);
                if let Some(ref name) = r.persona_name {
                    query = query.bind(name);
                }
                if let Some(ref steamid) = r.steamid {
                    query = query.bind(steamid);
                }
                if let Some(ref steamid3) = r.steamid3 {
                    query = query.bind(steamid3);
                }
                if let Some(ref url) = r.profile_url {
                    query = query.bind(url);
                }

                query.execute(&db.pool).await?;
                updated_count += 1;
            }
            processed += 1;
        }

        if processed < total {
            tokio::time::sleep(std::time::Duration::from_millis(BATCH_DELAY_MS)).await;
        }
    }

    Ok(updated_count)
}

#[derive(Default)]
struct RefreshResult {
    persona_name: Option<String>,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
}

/// 刷新单条记录的Steam名称
pub async fn refresh_single_steam_name(
    db: &Database,
    resolver: &SteamResolver,
    id: uuid::Uuid,
) -> anyhow::Result<Option<String>> {
    let steamid64: Option<(String,)> = sqlx::query_as(
        r#"SELECT steamid64 FROM whitelist_requests WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await?;

    let Some((steamid64,)) = steamid64 else {
        anyhow::bail!("记录不存在");
    };

    let Some(profile) = resolver.fetch_profile(&steamid64).await.ok().flatten() else {
        anyhow::bail!("无法获取Steam资料");
    };

    sqlx::query(
        r#"UPDATE whitelist_requests SET steam_persona_name = $2, updated_at = now() WHERE id = $1"#,
    )
    .bind(id)
    .bind(&profile.persona_name)
    .execute(&db.pool)
    .await?;

    Ok(Some(profile.persona_name))
}

/// 批量刷新指定状态的记录的Steam名称
pub async fn refresh_steam_names_by_status(
    db: &Database,
    resolver: &SteamResolver,
    status: &str,
) -> anyhow::Result<usize> {
    const CONCURRENT_REQUESTS: usize = 5;
    const BATCH_DELAY_MS: u64 = 500;

    let steamids: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        r#"SELECT id, steamid64 FROM whitelist_requests WHERE status = $1"#,
    )
    .bind(status)
    .fetch_all(&db.pool)
    .await?;

    if steamids.is_empty() {
        return Ok(0);
    }

    let total = steamids.len();
    let mut updated_count = 0;
    let mut processed = 0;

    for chunk in steamids.chunks(CONCURRENT_REQUESTS) {
        let results: Vec<(uuid::Uuid, Option<String>)> = stream::iter(chunk.iter().cloned())
            .then(|(id, steamid64)| async move {
                let profile = resolver.fetch_profile(&steamid64).await.ok().flatten();
                (id, profile.map(|p| p.persona_name))
            })
            .collect()
            .await;

        for (id, persona_name) in results {
            if let Some(name) = persona_name {
                sqlx::query(
                    r#"UPDATE whitelist_requests SET steam_persona_name = $2, updated_at = now() WHERE id = $1"#,
                )
                .bind(id)
                .bind(&name)
                .execute(&db.pool)
                .await?;
                updated_count += 1;
            }
            processed += 1;
        }

        if processed < total {
            tokio::time::sleep(std::time::Duration::from_millis(BATCH_DELAY_MS)).await;
        }
    }

    Ok(updated_count)
}
