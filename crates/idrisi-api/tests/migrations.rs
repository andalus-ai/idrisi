use sqlx::{postgres::PgPoolOptions, Row};
use std::env;
use uuid::Uuid;

#[tokio::test]
async fn postgres_migrations_and_staging_role_boundaries() {
    let Ok(database_url) = env::var("IDRISI_TEST_DATABASE_URL") else {
        eprintln!("skipping PostGIS integration test: IDRISI_TEST_DATABASE_URL is unset");
        return;
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to PostGIS integration database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("apply the complete Idrisi migration chain");

    let postgis_version: String = sqlx::query_scalar("SELECT PostGIS_Version()")
        .fetch_one(&pool)
        .await
        .expect("PostGIS extension is installed");
    assert!(!postgis_version.is_empty());

    let boundaries = sqlx::query(
        "SELECT
            has_schema_privilege('idrisi_ingest', 'tiles', 'USAGE') AS ingest_tiles_usage,
            has_schema_privilege('idrisi_ingest', 'tiles', 'CREATE') AS ingest_tiles_create,
            has_function_privilege(
                'idrisi_ingest',
                'idrisi_create_osm_staging_schemas(uuid)',
                'EXECUTE'
            ) AS ingest_can_create_staging,
            has_function_privilege(
                'idrisi_publish',
                'idrisi_publish_osm_dataset(uuid, uuid)',
                'EXECUTE'
            ) AS publisher_can_publish",
    )
    .fetch_one(&pool)
    .await
    .expect("read role boundaries");

    assert!(!boundaries.get::<bool, _>("ingest_tiles_usage"));
    assert!(!boundaries.get::<bool, _>("ingest_tiles_create"));
    assert!(boundaries.get::<bool, _>("ingest_can_create_staging"));
    assert!(boundaries.get::<bool, _>("publisher_can_publish"));

    let lease_token = Uuid::new_v4();
    let (data_schema, middle_schema): (String, String) =
        sqlx::query_as("SELECT * FROM idrisi_create_osm_staging_schemas($1)")
            .bind(lease_token)
            .fetch_one(&pool)
            .await
            .expect("create lease-derived staging schemas");

    let expected_suffix = lease_token.simple().to_string();
    assert_eq!(data_schema, format!("idrisi_stage_{expected_suffix}"));
    assert_eq!(middle_schema, format!("idrisi_mid_{expected_suffix}"));

    let mut connection = pool.acquire().await.expect("acquire test connection");
    sqlx::query("SET ROLE idrisi_ingest")
        .execute(&mut *connection)
        .await
        .expect("assume ingestion role");
    sqlx::query(&format!(
        "CREATE TABLE {data_schema}.permission_probe (id integer PRIMARY KEY)"
    ))
    .execute(&mut *connection)
    .await
    .expect("ingestion role owns its staging schema");
    sqlx::query("RESET ROLE")
        .execute(&mut *connection)
        .await
        .expect("reset integration role");

    let publisher_can_read: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('idrisi_publish', $1, 'SELECT')",
    )
    .bind(format!("{data_schema}.permission_probe"))
    .fetch_one(&mut *connection)
    .await
    .expect("check publisher default privileges");
    assert!(publisher_can_read);

    sqlx::query("SELECT idrisi_drop_osm_staging_schemas($1)")
        .bind(lease_token)
        .execute(&pool)
        .await
        .expect("drop test staging schemas");

    pool.close().await;
}
