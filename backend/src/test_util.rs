use sqlx::postgres::PgPoolOptions;

pub fn schema_url(base_url: &str, schema: &str) -> String {
    let separator = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{separator}options=-csearch_path%3D{schema}")
}

pub async fn create_schema(base_url: &str, schema: &str) {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(base_url)
        .await
        .expect("failed to connect to database for schema creation");
    sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#))
        .execute(&pool)
        .await
        .expect("failed to create schema");
    pool.close().await;
}

pub async fn drop_schema(base_url: &str, schema: &str) {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(base_url)
        .await
        .expect("failed to connect to database for schema drop");
    sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#))
        .execute(&pool)
        .await
        .expect("failed to drop schema");
    pool.close().await;
}
