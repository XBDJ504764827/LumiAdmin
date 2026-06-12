/// 共享的 SQL 片段，用于将操作员的显示名解析为管理员优先显示。
///
/// 匹配逻辑：优先匹配 username，其次 display_name，最后 remark。
/// 结果通过 COALESCE 在 SELECT 中使用：
/// `COALESCE(operator_user.display_name, br.operator_name) AS operator_name`

/// 操作员的 LEFT JOIN LATERAL 片段（用于 operator_name 字段）
pub const OPERATOR_DISPLAY_JOIN: &str = r#"LEFT JOIN LATERAL (
    SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
    FROM users u
    WHERE u.username = br.operator_name
       OR u.display_name = br.operator_name
       OR NULLIF(u.remark, '') = br.operator_name
    ORDER BY CASE WHEN u.username = br.operator_name THEN 0 WHEN u.display_name = br.operator_name THEN 1 ELSE 2 END
    LIMIT 1
) operator_user ON true"#;

/// 解封操作员的 LEFT JOIN LATERAL 片段（用于 removed_by 字段）
pub const REMOVED_BY_DISPLAY_JOIN: &str = r#"LEFT JOIN LATERAL (
    SELECT COALESCE(NULLIF(u.remark, ''), u.username) AS display_name
    FROM users u
    WHERE u.username = br.removed_by
       OR u.display_name = br.removed_by
       OR NULLIF(u.remark, '') = br.removed_by
    ORDER BY CASE WHEN u.username = br.removed_by THEN 0 WHEN u.display_name = br.removed_by THEN 1 ELSE 2 END
    LIMIT 1
) removed_user ON true"#;
