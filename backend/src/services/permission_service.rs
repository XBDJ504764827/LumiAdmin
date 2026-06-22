use crate::{
    auth::permissions::{ROLE_ADMIN, ROLE_DEVELOPER, ROLE_NORMAL},
    models::Operator,
    services::ban_service::BanRecord,
};

pub fn can_manage_user(actor: &Operator, target: &crate::models::User) -> bool {
    match actor.role.as_str() {
        ROLE_DEVELOPER => true,
        ROLE_ADMIN => target.role != ROLE_DEVELOPER,
        ROLE_NORMAL => actor.id == target.id,
        _ => false,
    }
}

pub fn can_change_user_role(actor: &Operator, target: &crate::models::User) -> bool {
    match actor.role.as_str() {
        ROLE_DEVELOPER => true,
        ROLE_ADMIN => actor.id != target.id && target.role != ROLE_DEVELOPER,
        _ => false,
    }
}

pub fn can_delete_user(actor: &Operator, target: &crate::models::User) -> bool {
    if actor.id == target.id {
        return false;
    }

    match actor.role.as_str() {
        ROLE_DEVELOPER => true,
        ROLE_ADMIN => target.role != ROLE_DEVELOPER,
        _ => false,
    }
}

pub fn can_create_admin_user(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_manage_whitelist_manually(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_create_ban(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_review_ban_appeals(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_review_player_reports(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_review_whitelist(actor: &Operator) -> bool {
    matches!(
        actor.role.as_str(),
        ROLE_DEVELOPER | ROLE_ADMIN | ROLE_NORMAL
    )
}

pub fn can_revoke_whitelist(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_manage_community_mutation(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_manage_server_report_token(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_manage_player_api_config(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_unban_record(actor: &Operator, record: &BanRecord) -> bool {
    match actor.role.as_str() {
        ROLE_DEVELOPER | ROLE_ADMIN => true,
        ROLE_NORMAL => {
            record.operator_name == actor.display_name || record.operator_name == actor.username
        }
        _ => false,
    }
}

pub fn can_toggle_user_enabled(actor: &Operator, target: &crate::models::User) -> bool {
    if actor.id == target.id {
        return false;
    }
    match actor.role.as_str() {
        ROLE_DEVELOPER => true,
        ROLE_ADMIN => target.role != ROLE_DEVELOPER,
        _ => false,
    }
}

pub fn can_view_audit_logs(actor: &Operator) -> bool {
    matches!(
        actor.role.as_str(),
        ROLE_DEVELOPER | ROLE_ADMIN | ROLE_NORMAL
    )
}

/// 查看进服监控：含详细 IP/Rating/Steam 等级，只有 developer/admin 可看
pub fn can_view_access_logs(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}

pub fn can_manage_player_internal_data(actor: &Operator) -> bool {
    matches!(actor.role.as_str(), ROLE_DEVELOPER | ROLE_ADMIN)
}
