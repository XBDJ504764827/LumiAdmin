use crate::config::Config;
use crate::db::Database;
use crate::services::observability_service;
use crate::services::steam_service::SteamResolver;
use futures::stream::{self, StreamExt};
/// 每隔 interval_seconds 秒刷新所有白名单记录的Steam名称
pub fn start_steam_name_refresh_loop(db: Database, config: Config, interval_seconds: u64) {
    observability_service::register_task(
        "steam_name_refresh",
        "Steam 资料刷新",
        "外部依赖",
        Some(interval_seconds),
        true,
    );
    tokio::spawn(async move {
        // 首次启动延迟30秒，避免与其他初始化任务冲突
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
        loop {
            interval.tick().await;
            let resolver = SteamResolver::new(&config);
            match observability_service::observe_task(
                "steam_name_refresh",
                refresh_steam_names(&db, &resolver),
                |count| format!("刷新 {} 条 Steam 资料", count),
            )
            .await
            {
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
                if let Some(profile) = resolver
                    .fetch_profile(&record.steamid64)
                    .await
                    .ok()
                    .flatten()
                {
                    result.persona_name = Some(profile.persona_name);
                }

                // 本地计算缺失的 SteamID2/SteamID3/ProfileURL
                if let Ok(identity) = resolver.parse_local(&record.steamid64) {
                    if record
                        .steamid
                        .as_deref()
                        .is_none_or(|v| v.trim().is_empty())
                    {
                        result.steamid = identity.steamid;
                    }
                    if record
                        .steamid3
                        .as_deref()
                        .is_none_or(|v| v.trim().is_empty())
                    {
                        result.steamid3 = identity.steamid3;
                    }
                    if record
                        .profile_url
                        .as_deref()
                        .is_none_or(|v| v.trim().is_empty())
                    {
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

        // 收集本批次有更新的记录（任一字段非空即纳入），用一条批量 UPDATE 提交
        let mut ids: Vec<uuid::Uuid> = Vec::new();
        let mut persona_names: Vec<Option<String>> = Vec::new();
        let mut steamids: Vec<Option<String>> = Vec::new();
        let mut steamid3s: Vec<Option<String>> = Vec::new();
        let mut profile_urls: Vec<Option<String>> = Vec::new();

        for (id, result) in results {
            if let Some(r) = result {
                let has_update = r.persona_name.is_some()
                    || r.steamid.is_some()
                    || r.steamid3.is_some()
                    || r.profile_url.is_some();
                if has_update {
                    ids.push(id);
                    persona_names.push(r.persona_name);
                    steamids.push(r.steamid);
                    steamid3s.push(r.steamid3);
                    profile_urls.push(r.profile_url);
                }
            }
            processed += 1;
        }

        if !ids.is_empty() {
            // 缺失字段传 NULL，用 COALESCE 保留原值，语义等价于原“只 SET 有值的字段”
            let affected = sqlx::query(
                r#"UPDATE whitelist_requests AS w
                   SET steam_persona_name = COALESCE(d.persona_name, w.steam_persona_name),
                       steamid = COALESCE(d.steamid, w.steamid),
                       steamid3 = COALESCE(d.steamid3, w.steamid3),
                       profile_url = COALESCE(d.profile_url, w.profile_url),
                       updated_at = now()
                   FROM UNNEST($1::uuid[], $2::text[], $3::text[], $4::text[], $5::text[])
                        AS d(id, persona_name, steamid, steamid3, profile_url)
                   WHERE w.id = d.id"#,
            )
            .bind(&ids)
            .bind(&persona_names)
            .bind(&steamids)
            .bind(&steamid3s)
            .bind(&profile_urls)
            .execute(&db.pool)
            .await?
            .rows_affected();
            updated_count += affected as usize;
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
#[allow(dead_code)]
pub async fn refresh_single_steam_name(
    db: &Database,
    resolver: &SteamResolver,
    id: uuid::Uuid,
) -> anyhow::Result<Option<String>> {
    let steamid64: Option<(String,)> =
        sqlx::query_as(r#"SELECT steamid64 FROM whitelist_requests WHERE id = $1"#)
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
#[allow(dead_code)]
pub async fn refresh_steam_names_by_status(
    db: &Database,
    resolver: &SteamResolver,
    status: &str,
) -> anyhow::Result<usize> {
    const CONCURRENT_REQUESTS: usize = 5;
    const BATCH_DELAY_MS: u64 = 500;

    let steamids: Vec<(uuid::Uuid, String)> =
        sqlx::query_as(r#"SELECT id, steamid64 FROM whitelist_requests WHERE status = $1"#)
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

        // 收集本批次成功获取名称的记录，用一条批量 UPDATE 提交（替代逐条 UPDATE）
        let mut ids: Vec<uuid::Uuid> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for (id, persona_name) in results {
            if let Some(name) = persona_name {
                ids.push(id);
                names.push(name);
            }
            processed += 1;
        }

        if !ids.is_empty() {
            let affected = sqlx::query(
                r#"UPDATE whitelist_requests AS w
                   SET steam_persona_name = d.name, updated_at = now()
                   FROM UNNEST($1::uuid[], $2::text[]) AS d(id, name)
                   WHERE w.id = d.id"#,
            )
            .bind(&ids)
            .bind(&names)
            .execute(&db.pool)
            .await?
            .rows_affected();
            updated_count += affected as usize;
        }

        if processed < total {
            tokio::time::sleep(std::time::Duration::from_millis(BATCH_DELAY_MS)).await;
        }
    }

    Ok(updated_count)
}
