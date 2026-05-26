pub mod access_service;
pub mod access_snapshot_service;
pub mod audit_service;
pub mod auth_service;
pub mod ban_appeal_service;
pub mod ban_expiry_service;
pub mod ban_service;
pub mod community_service;
pub mod dashboard_service;
pub mod docs_service;
pub mod external_server_service;
pub mod log_service;
pub mod map_tier_service;
pub mod notification_service;
pub mod offline_sync_service;
pub mod permission_service;
pub mod player_access_rule_service;
pub mod player_api_service;
pub mod plugin_ban_service;
pub mod public_service;
pub mod r2_storage;
pub mod rate_limit_service;
pub mod rcon_poll_service;
pub mod server_config_cache;
pub mod server_status_service;
pub mod steam_name_refresh_service;
pub mod steam_service;
pub mod user_service;
pub mod whitelist_service;

/// Trim an optional string, returning None if empty after trimming.
pub fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

/// Trim an owned optional string in-place, returning None if empty after trimming.
pub fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}
