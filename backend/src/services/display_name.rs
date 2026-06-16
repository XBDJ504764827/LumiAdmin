//! 操作员显示名批量解析（应用层 JOIN）。
//!
//! 背景：`ban_records.operator_name` / `whitelist_requests.approved_by` 等字段
//! 存储的是字符串形式的操作员标识，可能对应 `users` 表的 `username`、
//! `display_name` 或 `remark` 之一。原先在列表查询里用 `LEFT JOIN LATERAL`
//! 对每一行都跑一次带 3 个 `OR` + `ORDER BY CASE` 的子查询，无法有效命中索引。
//!
//! 本模块改为：先取出本页所有需要解析的原始字符串，再用**一次**查询批量获取
//! 候选用户，在 Rust 侧按优先级（username > display_name > remark）解析，
//! 显著减少数据库往返与逐行子查询开销。

use crate::db::Database;
use std::collections::{HashMap, HashSet};

/// 优先级数值越小越优先：username(0) > display_name(1) > remark(2)。
const PRIO_USERNAME: u8 = 0;
const PRIO_DISPLAY_NAME: u8 = 1;
const PRIO_REMARK: u8 = 2;

/// 给定一组原始操作员标识字符串，返回「原始标识 -> 解析后的显示名」映射。
///
/// 解析规则与历史 LATERAL 子查询保持一致：
/// - 匹配维度优先级：`username` > `display_name` > `remark`
/// - 显示名取值：`COALESCE(NULLIF(remark, ''), username)`
/// - 未匹配到任何用户的原始标识不会出现在返回 map 中（调用方应回退到原值）
///
/// `names` 为空时返回空 map，不会发起数据库查询。
pub async fn resolve_display_names(
    db: &Database,
    names: &[String],
) -> anyhow::Result<HashMap<String, String>> {
    let unique: HashSet<String> = names
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if unique.is_empty() {
        return Ok(HashMap::new());
    }

    let arr: Vec<&str> = unique.iter().map(|s| s.as_str()).collect();
    // 一次查询拿到所有候选用户（匹配 username / display_name / 去空 remark 任意一列）
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        r#"SELECT username, display_name, remark
           FROM users
           WHERE username = ANY($1)
              OR display_name = ANY($1)
              OR NULLIF(remark, '') = ANY($1)"#,
    )
    .bind(&arr)
    .fetch_all(&db.pool)
    .await?;

    // 原始标识 -> (当前最佳优先级, 显示名)
    let mut best: HashMap<String, (u8, String)> = HashMap::new();
    for (username, display_name, remark) in rows {
        let remark_ne: Option<&str> = remark.as_deref().map(str::trim).filter(|r| !r.is_empty());
        let display = remark_ne.unwrap_or(&username).to_string();

        // 候选维度：(列值, 优先级)，按 username > display_name > remark 收集
        let mut candidates: Vec<(&str, u8)> = Vec::with_capacity(3);
        candidates.push((username.as_str(), PRIO_USERNAME));
        candidates.push((display_name.as_str(), PRIO_DISPLAY_NAME));
        if let Some(r) = remark_ne {
            candidates.push((r, PRIO_REMARK));
        }

        for (val, prio) in candidates {
            if unique.contains(val) {
                let should_update = match best.get(val) {
                    Some((current_prio, _)) => *current_prio > prio,
                    None => true,
                };
                if should_update {
                    best.insert(val.to_string(), (prio, display.clone()));
                }
            }
        }
    }

    Ok(best.into_iter().map(|(k, (_, v))| (k, v)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database};
    use uuid::Uuid;

    #[test]
    fn priority_constants_are_ordered() {
        assert!(PRIO_USERNAME < PRIO_DISPLAY_NAME);
        assert!(PRIO_DISPLAY_NAME < PRIO_REMARK);
    }

    /// 数据库集成测试：验证批量解析能正确按优先级（username > display_name > remark）
    /// 解析多个原始标识，且显示名取 COALESCE(NULLIF(remark,''), username)。
    #[tokio::test]
    async fn resolve_display_names_prefers_username_and_uses_remark() {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = crate::test_util::schema_url(&base_url, &schema);
        crate::test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;

            // 用户 A：username='alpha'，display_name='Alpha展示'，remark=''  -> 显示名应为 'alpha'
            // 用户 B：username='beta'，display_name='Beta展示'，remark='Beta备注' -> 显示名应为 'Beta备注'
            for (uid, username, display_name, remark) in [
                ("aaaa1111-0000-0000-0000-000000000001", "alpha", "Alpha展示", ""),
                ("aaaa1111-0000-0000-0000-000000000002", "beta", "Beta展示", "Beta备注"),
            ] {
                sqlx::query(
                    r#"INSERT INTO users (id, username, display_name, password_hash, role, remark)
                       VALUES ($1, $2, $3, 'x', 'normal', NULLIF($4, ''))"#,
                )
                .bind(Uuid::parse_str(uid).unwrap())
                .bind(username)
                .bind(display_name)
                .bind(remark)
                .execute(&db.pool)
                .await?;
            }

            // 用 display_name 列的值去查（验证优先级：应匹配到该用户）
            let names = vec![
                "alpha".to_string(),        // 命中 A.username
                "Alpha展示".to_string(),    // 命中 A.display_name（优先级低于 username，但同用户结果一致）
                "Beta展示".to_string(),     // 命中 B.display_name
                "nobody".to_string(),       // 任何列都不匹配
            ];
            let map = resolve_display_names(&db, &names).await?;

            // remark 为空 -> 显示名回落到 username
            assert_eq!(map.get("alpha").map(|s| s.as_str()), Some("alpha"));
            assert_eq!(map.get("Alpha展示").map(|s| s.as_str()), Some("alpha"));
            // remark 非空 -> 显示名取 remark
            assert_eq!(map.get("Beta展示").map(|s| s.as_str()), Some("Beta备注"));
            // 未匹配的标识不应出现在结果中
            assert!(!map.contains_key("nobody"));
            anyhow::Ok(())
        }
        .await;

        crate::test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }
}
