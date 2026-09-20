use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("password hash failed: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

/// 校验密码强度：至少 8 位，且必须同时包含字母和数字。
/// 返回 None 表示通过，Some(原因) 表示不通过。
pub fn validate_password_strength(password: &str) -> Option<&'static str> {
    let chars: Vec<char> = password.chars().collect();
    if chars.len() < 8 {
        return Some("密码长度至少为 8 位");
    }
    if chars.len() > 128 {
        return Some("密码长度不能超过 128 位");
    }
    let has_letter = chars.iter().any(|c| c.is_alphabetic());
    let has_digit = chars.iter().any(|c| c.is_ascii_digit());
    if !has_letter || !has_digit {
        return Some("密码必须同时包含字母和数字");
    }
    None
}
