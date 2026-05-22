use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: String,
    pub steam_id: Option<String>,
    pub remark: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Operator {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub token: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub display_name: String,
    pub role_label: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PublicWhitelistItem {
    pub id: Uuid,
    pub nickname: String,
    pub steamid64: String,
    pub applied_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PublicBanItem {
    pub id: Uuid,
    pub player: String,
    pub steam_id: String,
    pub server_name: Option<String>,
    pub duration_minutes: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub reason: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionResponse {
    pub session: SessionView,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionView {
    pub token: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub role: String,
    pub role_label: String,
}

impl From<Session> for SessionView {
    fn from(value: Session) -> Self {
        Self {
            token: value.token,
            user_id: value.user_id,
            display_name: value.display_name,
            role: value.role,
            role_label: value.role_label,
        }
    }
}
