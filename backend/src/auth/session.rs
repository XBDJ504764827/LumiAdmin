use crate::models::{Session, SessionView, User};
use chrono::{Duration, Utc};
use uuid::Uuid;

pub fn build_session(user: &User, ttl_hours: i64) -> Session {
    let now = Utc::now();
    Session {
        token: Uuid::new_v4(),
        user_id: user.id,
        role: user.role.clone(),
        display_name: user.display_name.clone(),
        role_label: role_label(&user.role).to_string(),
        expires_at: now + Duration::hours(ttl_hours),
        created_at: now,
    }
}

pub fn role_label(role: &str) -> &'static str {
    match role {
        "developer" => "开发管理员",
        "admin" => "系统管理员",
        "normal" => "普通管理员",
        _ => "游客",
    }
}

pub fn to_view(session: Session) -> SessionView {
    SessionView::from(session)
}

#[cfg(test)]
mod tests {
    use super::role_label;

    #[test]
    fn role_label_supports_normal_admin() {
        assert_eq!(role_label("developer"), "开发管理员");
        assert_eq!(role_label("admin"), "系统管理员");
        assert_eq!(role_label("normal"), "普通管理员");
    }
}
