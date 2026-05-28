use crate::{
    db::Database,
    routes::ListQuery,
    services::steam_service::{ParsedSteamIdentity, SteamResolver},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct WhitelistItem {
    pub id: Uuid,
    pub steamid64: String,
    pub steamid: Option<String>,
    pub steamid3: Option<String>,
    pub profile_url: Option<String>,
    pub nickname: String,
    pub steam_persona_name: Option<String>,
    pub status: String,
    pub applied_at: String,
    pub approved_at: Option<String>,
    pub approved_by: Option<String>,
    pub approval_reason: Option<String>,
    pub rejected_at: Option<String>,
    pub rejected_by: Option<String>,
    pub rejection_reason: Option<String>,
}

#[derive(Clone)]
pub struct PublicWhitelistRequestInput {
    pub nickname: String,
    pub steam_input: String,
}

#[derive(Clone)]
pub struct ManualWhitelistInput {
    pub nickname: String,
    pub steam_input: String,
}

#[derive(sqlx::FromRow)]
struct WhitelistRow {
    id: Uuid,
    steamid64: String,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
    nickname: String,
    steam_persona_name: Option<String>,
    status: String,
    applied_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    approved_by: Option<String>,
    approval_reason: Option<String>,
    rejected_at: Option<DateTime<Utc>>,
    rejected_by: Option<String>,
    rejection_reason: Option<String>,
}

#[allow(dead_code)]
#[derive(sqlx::FromRow)]
struct WhitelistStatusRow {
    id: Uuid,
    steamid64: String,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
    nickname: String,
    steam_persona_name: Option<String>,
    status: String,
    applied_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    approved_by: Option<String>,
    approval_reason: Option<String>,
    rejected_at: Option<DateTime<Utc>>,
    rejected_by: Option<String>,
    rejection_reason: Option<String>,
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<String>,
    source: Option<String>,
}

pub async fn list_whitelist(
    db: &Database,
    query: &ListQuery,
) -> anyhow::Result<crate::routes::PaginatedResponse<WhitelistItem>> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;
    let search_pattern = query.search_pattern();

    if search_pattern.is_some() {
        conditions.push(format!(
            "(steamid64 ILIKE ${param_idx} OR nickname ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }
    if let Some(ref status) = query.status {
        if !status.trim().is_empty() {
            conditions.push(format!("status = ${param_idx}"));
            param_idx += 1;
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM whitelist_requests {where_clause}");
    let data_sql = format!(
        r#"SELECT wr.id, wr.steamid64, wr.steamid, wr.steamid3, wr.profile_url, wr.nickname, wr.steam_persona_name, wr.status,
                  wr.applied_at, wr.approved_at, COALESCE(approved_user.display_name, wr.approved_by) AS approved_by, wr.approval_reason,
                  wr.rejected_at, COALESCE(rejected_user.display_name, wr.rejected_by) AS rejected_by, wr.rejection_reason
           FROM whitelist_requests wr
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = wr.approved_by
                OR u.display_name = wr.approved_by
                OR NULLIF(u.remark, '') = wr.approved_by
             ORDER BY CASE WHEN u.username = wr.approved_by THEN 0 WHEN u.display_name = wr.approved_by THEN 1 ELSE 2 END
             LIMIT 1
           ) approved_user ON true
           LEFT JOIN LATERAL (
             SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
             FROM users u
             WHERE u.username = wr.rejected_by
                OR u.display_name = wr.rejected_by
                OR NULLIF(u.remark, '') = wr.rejected_by
             ORDER BY CASE WHEN u.username = wr.rejected_by THEN 0 WHEN u.display_name = wr.rejected_by THEN 1 ELSE 2 END
             LIMIT 1
           ) rejected_user ON true
           {where_clause} ORDER BY wr.applied_at DESC LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, WhitelistRow>(&data_sql);

    if let Some(ref pattern) = search_pattern {
        count_query = count_query.bind(pattern);
        data_query = data_query.bind(pattern);
    }
    if let Some(ref status) = query.status {
        if !status.trim().is_empty() {
            count_query = count_query.bind(status.trim());
            data_query = data_query.bind(status.trim());
        }
    }
    data_query = data_query.bind(query.page_size()).bind(query.offset());

    let total = count_query.fetch_one(&db.pool).await?;
    let items = data_query
        .fetch_all(&db.pool)
        .await?
        .into_iter()
        .map(map_whitelist_row)
        .collect();

    Ok(crate::routes::PaginatedResponse {
        items,
        total,
        page: query.page(),
        page_size: query.page_size(),
    })
}

pub async fn create_public_whitelist_request(
    db: &Database,
    input: PublicWhitelistRequestInput,
    resolver: &SteamResolver,
) -> anyhow::Result<WhitelistItem> {
    let nickname = input.nickname.trim();
    anyhow::ensure!(!nickname.is_empty(), "请输入玩家名称");

    let identity = resolver.resolve(&input.steam_input).await?;
    if let Some(existing) = find_by_steamid64(db, &identity.steamid64).await? {
        match existing.status.as_str() {
            "pending" => anyhow::bail!("该玩家白名单还在审核中"),
            "approved" => anyhow::bail!("该玩家白名单已通过，可以正常进入游戏"),
            "rejected" => anyhow::bail!(
                "该玩家白名单未通过：{}",
                existing
                    .rejection_reason
                    .unwrap_or_else(|| "未填写拒绝理由".to_string())
            ),
            "revoked" => {
                return reopen_revoked_whitelist(db, existing.id, nickname, &identity, resolver)
                    .await
            }
            _ => anyhow::bail!("白名单状态异常，无法重复申请"),
        }
    }

    // 尝试获取 Steam 名称（5秒超时，超时则留空，后续定时任务会补充）
    let steam_persona_name = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        resolver.fetch_profile(&identity.steamid64),
    )
    .await
    {
        Ok(Ok(Some(profile))) => Some(profile.persona_name),
        _ => None,
    };

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        INSERT INTO whitelist_requests (
            id, steam_id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
            applied_at, source, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', now(), 'public', now())
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by, approval_reason,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(identity.steamid.as_deref().unwrap_or(&identity.steamid64))
    .bind(&identity.steamid64)
    .bind(identity.steamid.as_deref())
    .bind(identity.steamid3.as_deref())
    .bind(identity.profile_url.as_deref())
    .bind(nickname)
    .bind(steam_persona_name.as_deref())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

pub async fn create_manual_whitelist(
    db: &Database,
    input: ManualWhitelistInput,
    operator_name: &str,
    resolver: &SteamResolver,
) -> anyhow::Result<WhitelistItem> {
    let nickname = input.nickname.trim();
    anyhow::ensure!(!nickname.is_empty(), "请输入玩家名称");
    anyhow::ensure!(!operator_name.trim().is_empty(), "缺少审核管理员信息");

    let identity = resolver.resolve(&input.steam_input).await?;
    if let Some(existing) = find_by_steamid64(db, &identity.steamid64).await? {
        anyhow::ensure!(
            existing.status == "revoked",
            "该玩家已有白名单记录，无法重复手动添加"
        );
        return approve_existing_record(
            db,
            existing.id,
            nickname,
            operator_name,
            &identity,
            resolver,
        )
        .await;
    }

    let steam_persona_name = resolver
        .fetch_profile(&identity.steamid64)
        .await
        .ok()
        .flatten()
        .map(|p| p.persona_name);

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        INSERT INTO whitelist_requests (
            id, steam_id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
            applied_at, approved_at, approved_by, source, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'approved', now(), now(), $9, 'manual', now())
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by, approval_reason,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(identity.steamid.as_deref().unwrap_or(&identity.steamid64))
    .bind(&identity.steamid64)
    .bind(identity.steamid.as_deref())
    .bind(identity.steamid3.as_deref())
    .bind(identity.profile_url.as_deref())
    .bind(nickname)
    .bind(steam_persona_name.as_deref())
    .bind(operator_name.trim())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

pub async fn approve_whitelist(
    db: &Database,
    id: Uuid,
    operator_name: &str,
    reason: Option<&str>,
) -> anyhow::Result<WhitelistItem> {
    let current = find_by_id(db, id).await?;
    anyhow::ensure!(current.status == "pending", "只有待审核记录可以通过");

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        UPDATE whitelist_requests
        SET status = 'approved',
            approved_at = now(),
            approved_by = $2,
            approval_reason = $3,
            rejected_at = NULL,
            rejected_by = NULL,
            rejection_reason = NULL,
            revoked_at = NULL,
            revoked_by = NULL,
            updated_at = now()
        WHERE id = $1
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by, approval_reason,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(id)
    .bind(operator_name.trim())
    .bind(reason.and_then(|r| if r.trim().is_empty() { None } else { Some(r.trim()) }))
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

pub async fn reject_whitelist(
    db: &Database,
    id: Uuid,
    reason: &str,
    operator_name: &str,
) -> anyhow::Result<WhitelistItem> {
    let current = find_by_id(db, id).await?;
    anyhow::ensure!(current.status == "pending", "只有待审核记录可以拒绝");
    anyhow::ensure!(!reason.trim().is_empty(), "请输入拒绝理由");

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        UPDATE whitelist_requests
        SET status = 'rejected',
            rejected_at = now(),
            rejected_by = $2,
            rejection_reason = $3,
            approved_at = NULL,
            approved_by = NULL,
            approval_reason = NULL,
            revoked_at = NULL,
            revoked_by = NULL,
            updated_at = now()
        WHERE id = $1
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by, approval_reason,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(id)
    .bind(operator_name.trim())
    .bind(reason.trim())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

pub async fn restore_whitelist(
    db: &Database,
    id: Uuid,
    operator_name: &str,
) -> anyhow::Result<WhitelistItem> {
    let current = find_by_id(db, id).await?;
    anyhow::ensure!(current.status == "rejected", "只有未通过记录可以恢复通过");

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        UPDATE whitelist_requests
        SET status = 'approved',
            approved_at = now(),
            approved_by = $2,
            approval_reason = NULL,
            rejected_at = NULL,
            rejected_by = NULL,
            rejection_reason = NULL,
            revoked_at = NULL,
            revoked_by = NULL,
            updated_at = now()
        WHERE id = $1
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by, approval_reason,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(id)
    .bind(operator_name.trim())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

pub async fn revoke_whitelist(
    db: &Database,
    id: Uuid,
    operator_name: &str,
) -> anyhow::Result<WhitelistItem> {
    let current = find_by_id(db, id).await?;
    anyhow::ensure!(current.status == "approved", "只有已通过记录可以删除审核");

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        UPDATE whitelist_requests
        SET status = 'revoked',
            revoked_at = now(),
            revoked_by = $2,
            updated_at = now()
        WHERE id = $1
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(id)
    .bind(operator_name.trim())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

async fn reopen_revoked_whitelist(
    db: &Database,
    id: Uuid,
    nickname: &str,
    identity: &ParsedSteamIdentity,
    resolver: &SteamResolver,
) -> anyhow::Result<WhitelistItem> {
    let steam_persona_name = resolver
        .fetch_profile(&identity.steamid64)
        .await
        .ok()
        .flatten()
        .map(|p| p.persona_name);

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        UPDATE whitelist_requests
        SET nickname = $2,
            steamid = $3,
            steamid3 = $4,
            profile_url = $5,
            steam_persona_name = $6,
            status = 'pending',
            applied_at = now(),
            approved_at = NULL,
            approved_by = NULL,
            approval_reason = NULL,
            rejected_at = NULL,
            rejected_by = NULL,
            rejection_reason = NULL,
            revoked_at = NULL,
            revoked_by = NULL,
            source = 'public',
            updated_at = now()
        WHERE id = $1
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by, approval_reason,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(id)
    .bind(nickname)
    .bind(identity.steamid.as_deref())
    .bind(identity.steamid3.as_deref())
    .bind(identity.profile_url.as_deref())
    .bind(steam_persona_name.as_deref())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

async fn approve_existing_record(
    db: &Database,
    id: Uuid,
    nickname: &str,
    operator_name: &str,
    identity: &ParsedSteamIdentity,
    resolver: &SteamResolver,
) -> anyhow::Result<WhitelistItem> {
    let steam_persona_name = resolver
        .fetch_profile(&identity.steamid64)
        .await
        .ok()
        .flatten()
        .map(|p| p.persona_name);

    let row = sqlx::query_as::<_, WhitelistRow>(
        r#"
        UPDATE whitelist_requests
        SET nickname = $2,
            steamid = $3,
            steamid3 = $4,
            profile_url = $5,
            steam_persona_name = $6,
            status = 'approved',
            applied_at = now(),
            approved_at = now(),
            approved_by = $7,
            approval_reason = NULL,
            rejected_at = NULL,
            rejected_by = NULL,
            rejection_reason = NULL,
            revoked_at = NULL,
            revoked_by = NULL,
            source = 'manual',
            updated_at = now()
        WHERE id = $1
        RETURNING id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
                  applied_at, approved_at, approved_by, approval_reason,
                  rejected_at, rejected_by, rejection_reason
        "#,
    )
    .bind(id)
    .bind(nickname)
    .bind(identity.steamid.as_deref())
    .bind(identity.steamid3.as_deref())
    .bind(identity.profile_url.as_deref())
    .bind(steam_persona_name.as_deref())
    .bind(operator_name.trim())
    .fetch_one(&db.pool)
    .await?;

    Ok(map_whitelist_row(row))
}

async fn find_by_steamid64(
    db: &Database,
    steamid64: &str,
) -> anyhow::Result<Option<WhitelistStatusRow>> {
    sqlx::query_as::<_, WhitelistStatusRow>(
        r#"
        SELECT id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
               applied_at, approved_at, approved_by, approval_reason,
               rejected_at, rejected_by, rejection_reason,
               revoked_at, revoked_by, source
        FROM whitelist_requests
        WHERE steamid64 = $1
        ORDER BY COALESCE(updated_at, revoked_at, rejected_at, approved_at, applied_at) DESC,
                 applied_at DESC,
                 id DESC
        LIMIT 1
        "#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .map_err(Into::into)
}

async fn find_by_id(db: &Database, id: Uuid) -> anyhow::Result<WhitelistStatusRow> {
    sqlx::query_as::<_, WhitelistStatusRow>(
        r#"
        SELECT id, steamid64, steamid, steamid3, profile_url, nickname, steam_persona_name, status,
               applied_at, approved_at, approved_by, approval_reason,
               rejected_at, rejected_by, rejection_reason,
               revoked_at, revoked_by, source
        FROM whitelist_requests
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .map_err(Into::into)
}

/// 更新单条白名单记录的Steam名称
pub async fn update_steam_persona_name(
    db: &Database,
    id: Uuid,
    resolver: &SteamResolver,
) -> anyhow::Result<Option<String>> {
    let record = find_by_id(db, id).await?;
    let steam_persona_name = match resolver
        .fetch_profile(&record.steamid64)
        .await
        .ok()
        .flatten()
    {
        Some(profile) => profile.persona_name,
        None => return Ok(None),
    };

    sqlx::query(
        r#"UPDATE whitelist_requests SET steam_persona_name = $2, updated_at = now() WHERE id = $1"#,
    )
    .bind(id)
    .bind(&steam_persona_name)
    .execute(&db.pool)
    .await?;

    Ok(Some(steam_persona_name))
}

/// 批量更新所有白名单记录的Steam名称
/// 返回成功更新的数量
pub async fn refresh_all_steam_persona_names(
    db: &Database,
    resolver: &SteamResolver,
    status_filter: Option<&str>,
) -> anyhow::Result<usize> {
    let status_condition = match status_filter {
        Some(_) => "WHERE status = $1".to_string(),
        None => String::new(),
    };

    let records: Vec<(Uuid, String)> = if let Some(status) = status_filter {
        sqlx::query_as(&format!(
            r#"SELECT id, steamid64 FROM whitelist_requests {status_condition}"#
        ))
        .bind(status)
        .fetch_all(&db.pool)
        .await?
    } else {
        sqlx::query_as(&format!(
            r#"SELECT id, steamid64 FROM whitelist_requests {status_condition}"#
        ))
        .fetch_all(&db.pool)
        .await?
    };

    if records.is_empty() {
        return Ok(0);
    }

    // 批量查询 Steam Profile
    let steamids: Vec<String> = records
        .iter()
        .map(|(_, steamid64)| steamid64.clone())
        .collect();
    let profiles = resolver.fetch_profiles_batch(&steamids).await?;

    // 批量更新（每批 500 条）
    let mut updated_count = 0;
    let mut ids: Vec<Uuid> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for (id, steamid64) in &records {
        if let Some(profile) = profiles.get(steamid64) {
            ids.push(*id);
            names.push(profile.persona_name.clone());
        }
        if ids.len() >= 500 {
            sqlx::query(
                r#"UPDATE whitelist_requests w
                   SET steam_persona_name = u.name, updated_at = now()
                   FROM UNNEST($1::UUID[], $2::TEXT[]) AS u(id, name)
                   WHERE w.id = u.id"#,
            )
            .bind(&ids)
            .bind(&names)
            .execute(&db.pool)
            .await?;
            updated_count += ids.len();
            ids.clear();
            names.clear();
        }
    }
    if !ids.is_empty() {
        sqlx::query(
            r#"UPDATE whitelist_requests w
               SET steam_persona_name = u.name, updated_at = now()
               FROM UNNEST($1::UUID[], $2::TEXT[]) AS u(id, name)
               WHERE w.id = u.id"#,
        )
        .bind(&ids)
        .bind(&names)
        .execute(&db.pool)
        .await?;
        updated_count += ids.len();
    }

    Ok(updated_count)
}

fn map_whitelist_row(row: WhitelistRow) -> WhitelistItem {
    WhitelistItem {
        id: row.id,
        steamid64: row.steamid64,
        steamid: row.steamid,
        steamid3: row.steamid3,
        profile_url: row.profile_url,
        nickname: row.nickname,
        steam_persona_name: row.steam_persona_name,
        status: row.status,
        applied_at: row.applied_at.to_rfc3339(),
        approved_at: row.approved_at.map(|value| value.to_rfc3339()),
        approved_by: row.approved_by,
        approval_reason: row.approval_reason,
        rejected_at: row.rejected_at.map(|value| value.to_rfc3339()),
        rejected_by: row.rejected_by,
        rejection_reason: row.rejection_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_manual_whitelist, create_public_whitelist_request, find_by_steamid64,
        reject_whitelist, restore_whitelist, ManualWhitelistInput, PublicWhitelistRequestInput,
    };
    use crate::{config::Config, db::Database, services::steam_service::SteamResolver};
    use chrono::{Duration, Utc};
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

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

    async fn with_test_db(test: impl AsyncFnOnce(Database) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url;
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            test(db).await
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    async fn insert_whitelist_record(
        db: &Database,
        steamid64: &str,
        status: &str,
        nickname: &str,
        rejection_reason: Option<&str>,
        applied_at: chrono::DateTime<Utc>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO whitelist_requests (
                id, steam_id, steamid64, steamid, steamid3, profile_url, nickname, status,
                applied_at, approved_at, approved_by, rejected_at, rejected_by,
                rejection_reason, source, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                    CASE WHEN $8 = 'approved' THEN now() ELSE NULL END,
                    CASE WHEN $8 = 'approved' THEN '管理员A' ELSE NULL END,
                    CASE WHEN $8 = 'rejected' THEN now() ELSE NULL END,
                    CASE WHEN $8 = 'rejected' THEN '管理员B' ELSE NULL END,
                    $10, 'public', now())
            "#,
        )
        .bind(id)
        .bind(format!("STEAM_TEST_{steamid64}"))
        .bind(steamid64)
        .bind(format!("STEAM_TEST_{steamid64}"))
        .bind(format!(
            "[U:1:{}]",
            steamid64.parse::<u64>().unwrap() - 76561197960265728
        ))
        .bind(format!("https://steamcommunity.com/profiles/{steamid64}"))
        .bind(nickname)
        .bind(status)
        .bind(applied_at)
        .bind(rejection_reason)
        .execute(&db.pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn create_whitelist_request_reports_pending_status_for_existing_pending_record() {
        with_test_db(async |db| {
            insert_whitelist_record(
                &db,
                "76561198000000001",
                "pending",
                "玩家甲",
                None,
                Utc::now(),
            )
            .await;

            let error = create_public_whitelist_request(
                &db,
                PublicWhitelistRequestInput {
                    nickname: "玩家甲".to_string(),
                    steam_input: "76561198000000001".to_string(),
                },
                &SteamResolver::for_tests(),
            )
            .await
            .unwrap_err();

            assert_eq!(error.to_string(), "该玩家白名单还在审核中");
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn create_whitelist_request_reports_approved_status_for_existing_approved_record() {
        with_test_db(async |db| {
            insert_whitelist_record(
                &db,
                "76561198000000002",
                "approved",
                "玩家乙",
                None,
                Utc::now(),
            )
            .await;

            let error = create_public_whitelist_request(
                &db,
                PublicWhitelistRequestInput {
                    nickname: "玩家乙".to_string(),
                    steam_input: "76561198000000002".to_string(),
                },
                &SteamResolver::for_tests(),
            )
            .await
            .unwrap_err();

            assert_eq!(error.to_string(), "该玩家白名单已通过，可以正常进入游戏");
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn create_whitelist_request_reports_rejected_reason_for_existing_rejected_record() {
        with_test_db(async |db| {
            insert_whitelist_record(
                &db,
                "76561198000000003",
                "rejected",
                "玩家丙",
                Some("资料不完整"),
                Utc::now(),
            )
            .await;

            let error = create_public_whitelist_request(
                &db,
                PublicWhitelistRequestInput {
                    nickname: "玩家丙".to_string(),
                    steam_input: "76561198000000003".to_string(),
                },
                &SteamResolver::for_tests(),
            )
            .await
            .unwrap_err();

            assert_eq!(error.to_string(), "该玩家白名单未通过：资料不完整");
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn create_whitelist_request_reopens_revoked_record_as_pending() {
        with_test_db(async |db| {
            insert_whitelist_record(
                &db,
                "76561198000000004",
                "revoked",
                "玩家丁",
                None,
                Utc::now() - Duration::days(3),
            )
            .await;

            let item = create_public_whitelist_request(
                &db,
                PublicWhitelistRequestInput {
                    nickname: "玩家丁".to_string(),
                    steam_input: "76561198000000004".to_string(),
                },
                &SteamResolver::for_tests(),
            )
            .await
            .unwrap();

            assert_eq!(item.status, "pending");
            assert_eq!(item.steamid64, "76561198000000004");
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn create_manual_whitelist_creates_approved_record() {
        with_test_db(async |db| {
            let item = create_manual_whitelist(
                &db,
                ManualWhitelistInput {
                    nickname: "管理员添加玩家".to_string(),
                    steam_input: "STEAM_0:1:12345".to_string(),
                },
                "Alex",
                &SteamResolver::for_tests(),
            )
            .await
            .unwrap();

            assert_eq!(item.status, "approved");
            assert_eq!(item.approved_by.as_deref(), Some("Alex"));
            assert_eq!(item.steamid.as_deref(), Some("STEAM_0:1:12345"));
            assert_eq!(item.steamid3.as_deref(), Some("[U:1:24691]"));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn reject_whitelist_requires_reason() {
        with_test_db(async |db| {
            let id = insert_whitelist_record(
                &db,
                "76561198000000011",
                "pending",
                "玩家戊",
                None,
                Utc::now(),
            )
            .await;
            let error = reject_whitelist(&db, id, "", "Alex").await.unwrap_err();
            assert_eq!(error.to_string(), "请输入拒绝理由");
            Ok(())
        })
        .await;
    }
    #[tokio::test]
    async fn restore_whitelist_preserves_original_applied_at() {
        with_test_db(async |db| {
            let applied_at = Utc::now() - Duration::days(5);
            let id = insert_whitelist_record(
                &db,
                "76561198000000012",
                "rejected",
                "玩家己",
                Some("资料不完整"),
                applied_at,
            )
            .await;

            let item = restore_whitelist(&db, id, "Alex").await.unwrap();

            assert_eq!(item.status, "approved");
            let applied_at_from_item = chrono::DateTime::parse_from_rfc3339(&item.applied_at)
                .unwrap()
                .with_timezone(&Utc);
            assert_eq!(
                applied_at_from_item.timestamp_micros(),
                applied_at.timestamp_micros()
            );
            assert_eq!(item.approved_by.as_deref(), Some("Alex"));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn create_whitelist_request_uses_latest_duplicate_pending_record() {
        with_test_db(async |db| {
            let older_applied_at = Utc::now() - Duration::hours(2);
            let newer_applied_at = Utc::now() - Duration::hours(1);
            let older_id = insert_whitelist_record(
                &db,
                "76561198000000021",
                "pending",
                "旧待审记录",
                None,
                older_applied_at,
            )
            .await;
            let newer_id = insert_whitelist_record(
                &db,
                "76561198000000021",
                "pending",
                "新待审记录",
                None,
                newer_applied_at,
            )
            .await;

            let row = find_by_steamid64(&db, "76561198000000021")
                .await?
                .expect("record should exist");

            assert_eq!(row.id, newer_id);
            assert_ne!(row.id, older_id);
            assert_eq!(row.nickname, "新待审记录");
            Ok(())
        })
        .await;
    }
}
