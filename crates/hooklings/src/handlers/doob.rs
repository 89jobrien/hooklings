//! `doob::pending` — query pending todos from the doob SQLite DB.

use cruxx_core::prelude::CruxErr;
use cruxx_script::HandlerRegistry;
use rusqlite::Connection;
use serde_json::{Value, json};

pub fn register(registry: &mut HandlerRegistry, db_path: &str) {
    let db = db_path.to_string();
    registry.handler_value("doob::pending", move |_input: Value| {
        let db = db.clone();
        async move {
            let conn = Connection::open(&db)
                .map_err(|e| CruxErr::step_failed("doob::pending", format!("open {db}: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, title, status, project, due_date FROM todos \
                     WHERE status = 'pending' ORDER BY due_date, id",
                )
                .map_err(|e| CruxErr::step_failed("doob::pending", format!("prepare: {e}")))?;

            let todos: Vec<Value> = stmt
                .query_map([], |row| {
                    Ok(json!({
                        "id": row.get::<_, String>(0).unwrap_or_default(),
                        "title": row.get::<_, String>(1).unwrap_or_default(),
                        "status": row.get::<_, String>(2).unwrap_or_default(),
                        "project": row.get::<_, Option<String>>(3).unwrap_or(None),
                        "due_date": row.get::<_, Option<String>>(4).unwrap_or(None),
                    }))
                })
                .map_err(|e| CruxErr::step_failed("doob::pending", format!("query: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            let count = todos.len();
            Ok(json!({ "todos": todos, "count": count }))
        }
    });
}
