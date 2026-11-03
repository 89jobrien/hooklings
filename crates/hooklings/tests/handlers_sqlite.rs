//! Integration tests for handoff and generic SQLite query handlers.

use crux_agentic::sqlite;
use crux_script::HandlerRegistry;
use hooklings::handlers::handoff;
use rusqlite::Connection;
use serde_json::json;
use tempfile::NamedTempFile;

#[allow(dead_code)]
fn setup_handoff_db() -> NamedTempFile {
    let f = NamedTempFile::new().unwrap();
    let conn = Connection::open(f.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE items (
            project TEXT NOT NULL,
            id TEXT NOT NULL,
            name TEXT,
            priority TEXT,
            status TEXT,
            completed TEXT,
            updated TEXT,
            PRIMARY KEY (project, id)
        );
        INSERT INTO items VALUES ('proj-a', 'a-1', 'open task', 'P1', 'open', NULL, NULL);
        INSERT INTO items VALUES ('proj-a', 'a-2', 'done task', 'P1', 'done', '2026-01-01', NULL);
        INSERT INTO items VALUES ('proj-b', 'b-1', 'another open', 'P0', 'open', NULL, NULL);",
    )
    .unwrap();
    f
}

#[allow(dead_code)]
fn setup_todos_db() -> NamedTempFile {
    let f = NamedTempFile::new().unwrap();
    let conn = Connection::open(f.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE todos (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            status TEXT NOT NULL,
            project TEXT,
            due_date TEXT
        );
        INSERT INTO todos VALUES ('t1', 'pending task', 'pending', 'hooklings', NULL);
        INSERT INTO todos VALUES ('t2', 'done task', 'done', 'hooklings', NULL);",
    )
    .unwrap();
    f
}

#[tokio::test]
async fn handoff_pending_returns_open_items_only() {
    let db = setup_handoff_db();
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 20);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({})).await.unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    for item in items {
        assert_ne!(item["status"], "done");
    }
}

#[tokio::test]
async fn handoff_pending_filter_by_project() {
    let db = setup_handoff_db();
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 20);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({"args": {"project": "proj-a"}})).await.unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["project"], "proj-a");
}

/// Build a DB with `n` open items so the cap can be exercised without fixtures.
fn setup_many_open_items(n: usize) -> NamedTempFile {
    let f = NamedTempFile::new().unwrap();
    let conn = Connection::open(f.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE items (
            project TEXT NOT NULL,
            id TEXT NOT NULL,
            name TEXT,
            priority TEXT,
            status TEXT,
            completed TEXT,
            updated TEXT,
            PRIMARY KEY (project, id)
        );",
    )
    .unwrap();
    for i in 0..n {
        conn.execute(
            "INSERT INTO items VALUES (?1, ?2, ?3, 'P1', 'open', NULL, NULL)",
            rusqlite::params![
                format!("proj-{}", i % 3),
                format!("id-{i:04}"),
                format!("task {i}")
            ],
        )
        .unwrap();
    }
    f
}

#[tokio::test]
async fn handoff_pending_caps_items_but_count_stays_truthful() {
    let db = setup_many_open_items(50);
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 20);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({})).await.unwrap();

    // Default cap applies...
    assert_eq!(result["items"].as_array().unwrap().len(), 20);
    assert_eq!(result["returned"], 20);
    // ...but count reports the real total so a caller can say "20 of 50".
    assert_eq!(result["count"], 50);
    assert_eq!(result["truncated"], true);
}

#[tokio::test]
async fn handoff_pending_limit_arg_overrides_default() {
    let db = setup_many_open_items(50);
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 20);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({"args": {"limit": 5}})).await.unwrap();

    assert_eq!(result["items"].as_array().unwrap().len(), 5);
    assert_eq!(result["count"], 50);
    assert_eq!(result["truncated"], true);
}

#[tokio::test]
async fn handoff_pending_under_limit_is_not_marked_truncated() {
    let db = setup_handoff_db();
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 20);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({"args": {"limit": 50}})).await.unwrap();

    assert_eq!(result["count"], 2);
    assert_eq!(result["returned"], 2);
    assert_eq!(result["truncated"], false);
}

#[tokio::test]
async fn handoff_pending_limit_composes_with_project_filter() {
    let db = setup_many_open_items(30);
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 20);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({"args": {"project": "proj-0", "limit": 2}}))
        .await
        .unwrap();

    // proj-0 holds 10 of the 30 items; the cap trims to 2 and count reflects the filter.
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    for item in result["items"].as_array().unwrap() {
        assert_eq!(item["project"], "proj-0");
    }
    assert_eq!(result["count"], 10);
    assert_eq!(result["truncated"], true);
}

#[tokio::test]
async fn handoff_pending_limit_is_clamped_not_trusted() {
    let db = setup_many_open_items(3);
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 20);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    // 0 would otherwise return nothing, hiding all pending work behind a bad argument.
    let result = h(json!({"args": {"limit": 0}})).await.unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["count"], 3);
}

#[tokio::test]
async fn handoff_pending_empty_db_returns_empty_array() {
    let f = NamedTempFile::new().unwrap();
    let conn = Connection::open(f.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE items (
            project TEXT NOT NULL, id TEXT NOT NULL, name TEXT,
            priority TEXT, status TEXT, completed TEXT, updated TEXT,
            PRIMARY KEY (project, id)
        );",
    )
    .unwrap();
    drop(conn);

    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, f.path().to_str().unwrap(), 20);
    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({})).await.unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn handoff_pending_uses_configured_default_limit() {
    let db = setup_many_open_items(30);
    let mut reg = HandlerRegistry::new();
    handoff::register(&mut reg, db.path().to_str().unwrap(), 7);

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({})).await.unwrap();

    assert_eq!(result["items"].as_array().unwrap().len(), 7);
    assert_eq!(result["returned"], 7);
    assert_eq!(result["count"], 30);
    assert_eq!(result["truncated"], true);
}

/// Demonstrate using sqlite::query_many directly for any SQLite-backed todo store.
/// This is the intended pattern — no doob-specific handler needed.
#[tokio::test]
async fn sqlite_query_many_for_todos() {
    let db = setup_todos_db();
    let mut reg = HandlerRegistry::new();
    sqlite::register(&mut reg);

    let h = reg.get_handler("sqlite::query_many").unwrap().clone();
    let result = h(json!({
        "args": {
            "db": db.path().to_str().unwrap(),
            "sql": "SELECT id, title, status FROM todos WHERE status = 'pending'"
        }
    }))
    .await
    .unwrap();

    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["status"], "pending");
}
