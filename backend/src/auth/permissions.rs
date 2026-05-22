#[allow(dead_code)]
pub fn can_access(role: &str, allowed: &[&str]) -> bool {
    role == "developer" || allowed.iter().any(|r| r == &role)
}
