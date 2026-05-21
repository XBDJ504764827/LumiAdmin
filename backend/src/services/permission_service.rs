use crate::{models::User, services::ban_service::BanRecord};

pub fn can_manage_user(actor: &User, target: &User) -> bool {
    match actor.role.as_str() {
        "developer" => true,
        "admin" => target.role != "developer",
        "normal" => actor.id == target.id,
        _ => false,
    }
}

pub fn can_change_user_role(actor: &User, target: &User) -> bool {
    match actor.role.as_str() {
        "developer" => true,
        "admin" => actor.id != target.id && target.role != "developer",
        _ => false,
    }
}

pub fn can_delete_user(actor: &User, target: &User) -> bool {
    if actor.id == target.id {
        return false;
    }

    match actor.role.as_str() {
        "developer" => true,
        "admin" => target.role != "developer",
        _ => false,
    }
}

pub fn can_create_admin_user(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin")
}

pub fn can_manage_whitelist_manually(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin")
}

pub fn can_review_whitelist(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin" | "normal")
}

pub fn can_revoke_whitelist(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin")
}

pub fn can_manage_community_mutation(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin")
}

pub fn can_manage_server_report_token(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin")
}

pub fn can_manage_player_api_config(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin")
}

pub fn can_unban_record(actor: &User, record: &BanRecord) -> bool {
    match actor.role.as_str() {
        "developer" | "admin" => true,
        "normal" => record.operator_name == actor.display_name,
        _ => false,
    }
}

pub fn can_toggle_user_enabled(actor: &User, target: &User) -> bool {
    if actor.id == target.id {
        return false;
    }
    match actor.role.as_str() {
        "developer" => true,
        "admin" => target.role != "developer",
        _ => false,
    }
}

pub fn can_view_audit_logs(actor: &User) -> bool {
    matches!(actor.role.as_str(), "developer" | "admin" | "normal")
}
