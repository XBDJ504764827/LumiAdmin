#[allow(dead_code)]
pub const ROLE_DEVELOPER: &str = "developer";
#[allow(dead_code)]
pub const ROLE_ADMIN: &str = "admin";
#[allow(dead_code)]
pub const ROLE_NORMAL: &str = "normal";
#[allow(dead_code)]
pub const ROLE_GUEST: &str = "guest";
#[allow(dead_code)]
pub const ADMIN_ROLES: [&str; 2] = [ROLE_DEVELOPER, ROLE_ADMIN];
#[allow(dead_code)]
pub const STAFF_ROLES: [&str; 3] = [ROLE_DEVELOPER, ROLE_ADMIN, ROLE_NORMAL];

#[allow(dead_code)]
pub fn can_access(role: &str, allowed: &[&str]) -> bool {
    role == ROLE_DEVELOPER || allowed.iter().any(|r| r == &role)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_can_access_any_role_gated_route() {
        assert!(can_access(ROLE_DEVELOPER, &[ROLE_NORMAL]));
    }

    #[test]
    fn admin_roles_do_not_include_normal() {
        assert_eq!(ADMIN_ROLES, [ROLE_DEVELOPER, ROLE_ADMIN]);
        assert!(!ADMIN_ROLES.contains(&ROLE_NORMAL));
    }
}
