//! Integration tests against a real Electric + Postgres instance.
//!
//! These tests require Docker services to be running per the AGENTS.md
//! instructions.  Start them with:
//!
//! ```sh
//! cd packages/sync-service/dev
//! docker compose -f docker-compose.yml -f docker-compose-electric.yml \
//!   up --wait postgres electric
//! ```
//!
//! Run with:
//! ```sh
//! cargo test --features integration
//! ```

mod support;

#[cfg(feature = "integration")]
mod tests {
    use super::support::*;
    use electric_client::{client::ShapeStreamOptions, shape::Shape, Message, ShapeStream};
    use std::time::Duration;
    use tokio_postgres::NoTls;

    const ELECTRIC_URL: &str = "http://localhost:3000/v1/shape";
    const PG_URL: &str =
        "host=localhost port=54321 user=postgres password=password dbname=electric";

    async fn connect_pg() -> tokio_postgres::Client {
        let (client, conn) = tokio_postgres::connect(PG_URL, NoTls)
            .await
            .expect("Failed to connect to Postgres");
        tokio::spawn(conn);
        client
    }

    async fn unique_table(pg: &tokio_postgres::Client, schema: &str) -> String {
        let name = format!(
            "rust_test_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "_")
        );
        pg.execute(
            &format!(
                "CREATE TABLE {schema}.{name} (
                    id   INTEGER PRIMARY KEY,
                    text TEXT NOT NULL
                )"
            ),
            &[],
        )
        .await
        .expect("CREATE TABLE");
        name
    }

    async fn wait_for_electric(url: &str, timeout: Duration) {
        let client = reqwest::Client::new();
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Ok(resp) = client
                .get(&format!(
                    "{}{}",
                    url.trim_end_matches("/v1/shape"),
                    "/v1/health"
                ))
                .send()
                .await
            {
                if resp.status().is_success() {
                    return;
                }
            }
            if std::time::Instant::now() > deadline {
                panic!("Electric did not become ready within {:?}", timeout);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    #[tokio::test]
    async fn initial_sync_from_postgres() {
        wait_for_electric(ELECTRIC_URL, Duration::from_secs(30)).await;

        let pg = connect_pg().await;
        let table = unique_table(&pg, "public").await;

        // Insert two rows
        pg.execute(
            &format!("INSERT INTO public.{table} (id, text) VALUES (1, 'hello'), (2, 'world')"),
            &[],
        )
        .await
        .unwrap();

        let stream = ShapeStream::new(ShapeStreamOptions {
            url: ELECTRIC_URL.to_string(),
            table: table.clone(),
            subscribe: false,
            ..Default::default()
        })
        .unwrap();

        let shape = Shape::new(stream);
        let rows = tokio::time::timeout(Duration::from_secs(15), shape.rows())
            .await
            .expect("timed out waiting for shape");

        assert_eq!(rows.len(), 2, "expected 2 rows, got {:?}", rows);

        // Cleanup
        pg.execute(&format!("DROP TABLE public.{table}"), &[])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn live_update_reflects_in_shape() {
        wait_for_electric(ELECTRIC_URL, Duration::from_secs(30)).await;

        let pg = connect_pg().await;
        let table = unique_table(&pg, "public").await;

        // Insert initial row
        pg.execute(
            &format!("INSERT INTO public.{table} (id, text) VALUES (10, 'initial')"),
            &[],
        )
        .await
        .unwrap();

        let stream = ShapeStream::new(ShapeStreamOptions {
            url: ELECTRIC_URL.to_string(),
            table: table.clone(),
            subscribe: true,
            ..Default::default()
        })
        .unwrap();

        let shape = Shape::new(stream);

        // Wait for initial snapshot
        let initial = tokio::time::timeout(Duration::from_secs(15), shape.rows())
            .await
            .expect("timed out");
        assert_eq!(initial.len(), 1);

        // Perform a live update
        pg.execute(
            &format!("UPDATE public.{table} SET text = 'updated' WHERE id = 10"),
            &[],
        )
        .await
        .unwrap();

        // Poll until the update is reflected (max 15s)
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let rows = shape.current_rows();
            if rows
                .first()
                .and_then(|r| r.get("text"))
                .and_then(|v| v.as_str())
                == Some("updated")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for update"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Cleanup
        pg.execute(&format!("DROP TABLE public.{table}"), &[])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn where_clause_filters_rows() {
        wait_for_electric(ELECTRIC_URL, Duration::from_secs(30)).await;

        let pg = connect_pg().await;
        let table = unique_table(&pg, "public").await;

        pg.execute(
            &format!(
                "INSERT INTO public.{table} (id, text)
                 VALUES (1, 'include'), (2, 'exclude'), (3, 'include')"
            ),
            &[],
        )
        .await
        .unwrap();

        let stream = ShapeStream::new(ShapeStreamOptions {
            url: ELECTRIC_URL.to_string(),
            table: table.clone(),
            where_clause: Some("text = 'include'".to_string()),
            subscribe: false,
            ..Default::default()
        })
        .unwrap();

        let shape = Shape::new(stream);
        let rows = tokio::time::timeout(Duration::from_secs(15), shape.rows())
            .await
            .expect("timed out");

        assert_eq!(rows.len(), 2);

        pg.execute(&format!("DROP TABLE public.{table}"), &[])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_is_reflected() {
        wait_for_electric(ELECTRIC_URL, Duration::from_secs(30)).await;

        let pg = connect_pg().await;
        let table = unique_table(&pg, "public").await;

        pg.execute(
            &format!("INSERT INTO public.{table} (id, text) VALUES (1, 'to delete'), (2, 'stay')"),
            &[],
        )
        .await
        .unwrap();

        let stream = ShapeStream::new(ShapeStreamOptions {
            url: ELECTRIC_URL.to_string(),
            table: table.clone(),
            subscribe: true,
            ..Default::default()
        })
        .unwrap();

        let shape = Shape::new(stream);

        // Wait for initial snapshot
        let _ = tokio::time::timeout(Duration::from_secs(15), shape.rows())
            .await
            .expect("timed out");

        // Delete one row
        pg.execute(&format!("DELETE FROM public.{table} WHERE id = 1"), &[])
            .await
            .unwrap();

        // Wait for the delete to propagate
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let rows = shape.current_rows();
            if rows.len() == 1 {
                assert_eq!(rows[0].get("text").and_then(|v| v.as_str()), Some("stay"));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for delete"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        pg.execute(&format!("DROP TABLE public.{table}"), &[])
            .await
            .unwrap();
    }
}
