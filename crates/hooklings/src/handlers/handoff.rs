//! `handoff::pending` — query open handoff items from the atelier SQLite DB.

use crux_runtime::prelude::CruxErr;
use crux_script::HandlerRegistry;
use rusqlite::Connection;
use serde_json::{Value, json};

pub fn register(registry: &mut HandlerRegistry, db_path: &str) {
    let db = db_path.to_string();
    registry.handler_value("handoff::pending", move |input: Value| {
        let db = db.clone();
        async move {
            let project_filter = input
                .get("args")
                .and_then(|a| a.get("project"))
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let conn = Connection::open(&db)
                .map_err(|e| CruxErr::step_failed("handoff::pending", format!("open {db}: {e}")))?;

            let (sql, params): (String, Vec<String>) = if let Some(proj) = project_filter {
                (
                    "SELECT project, id, name, priority, status FROM items \
                     WHERE status <> 'done' AND project = ?1 ORDER BY priority, id"
                        .into(),
                    vec![proj],
                )
            } else {
                (
                    "SELECT project, id, name, priority, status FROM items \
                     WHERE status <> 'done' ORDER BY priority, project, id"
                        .into(),
                    vec![],
                )
            };

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| CruxErr::step_failed("handoff::pending", format!("prepare: {e}")))?;

            let items: Vec<Value> = stmt
                .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                    Ok(json!({
                        "project": row.get::<_, String>(0).unwrap_or_default(),
                        "id": row.get::<_, String>(1).unwrap_or_default(),
                        "name": row.get::<_, Option<String>>(2).unwrap_or(None),
                        "priority": row.get::<_, Option<String>>(3).unwrap_or(None),
                        "status": row.get::<_, String>(4).unwrap_or_default(),
                    }))
                })
                .map_err(|e| CruxErr::step_failed("handoff::pending", format!("query: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            let count = items.len();
            Ok(json!({ "items": items, "count": count }))
        }
    });
}
