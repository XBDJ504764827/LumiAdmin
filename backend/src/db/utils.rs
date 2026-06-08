use super::Database;

impl Database {
    pub(super) async fn pg_trgm_schema(&self) -> Option<String> {
        if let Err(error) = sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm")
            .execute(&self.pool)
            .await
        {
            tracing::warn!(
                error = %error,
                "pg_trgm extension could not be created; checking whether it already exists"
            );
        }

        match sqlx::query_scalar::<_, String>(
            r#"SELECT n.nspname
               FROM pg_extension e
               JOIN pg_namespace n ON n.oid = e.extnamespace
               WHERE e.extname = 'pg_trgm'"#,
        )
        .fetch_one(&self.pool)
        .await
        {
            Ok(schema) => Some(schema),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "failed to check pg_trgm extension; skipping trigram search indexes"
                );
                None
            }
        }
    }
}

#[cfg(not(test))]
pub(super) fn quote_ident(identifier: &str) -> String {
    format!(r#""{}""#, identifier.replace('"', r#""""#))
}
