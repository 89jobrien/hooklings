use cruxx_script::HandlerRegistry;
use hooklings::handlers::{doob, handoff};
use rusqlite::Connection;
use serde_json::json;
use tempfile::NamedTempFile;

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

fn setup_doob_db() -> NamedTempFile {
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
    handoff::register(&mut reg, db.path().to_str().unwrap());

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
    handoff::register(&mut reg, db.path().to_str().unwrap());

    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({"args": {"project": "proj-a"}})).await.unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["project"], "proj-a");
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
    handoff::register(&mut reg, f.path().to_str().unwrap());
    let h = reg.get_handler("handoff::pending").unwrap().clone();
    let result = h(json!({})).await.unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn doob_pending_returns_pending_todos_only() {
    let db = setup_doob_db();
    let mut reg = HandlerRegistry::new();
    doob::register(&mut reg, db.path().to_str().unwrap());

    let h = reg.get_handler("doob::pending").unwrap().clone();
    let result = h(json!({})).await.unwrap();
    let todos = result["todos"].as_array().unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0]["status"], "pending");
}
