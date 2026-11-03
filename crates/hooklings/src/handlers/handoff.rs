//! `handoff::pending` — query open handoff items from the atelier SQLite DB.

use crux_runtime::prelude::CruxErr;
use crux_script::HandlerRegistry;
use rusqlite::{Connection, params_from_iter};
use serde_json::{Value, json};

/// Registers the handoff query handler against the SQLite database at `db_path`.
/// `default_limit` is the item count returned when the caller does not pass a
/// `limit` arg (see [`crate::config::HandoffConfig::default_limit`]).
pub fn register(registry: &mut HandlerRegistry, db_path: &str, default_limit: usize) {
    let db = db_path.to_string();
    registry.handler_value("handoff::pending", move |input: Value| {
        let db = db.clone();
        async move {
            let project_filter = input
                .get("args")
                .and_then(|a| a.get("project"))
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let limit = input
                .get("args")
                .and_then(|a| a.get("limit"))
                .and_then(|v| v.as_u64())
                .map(|l| l.clamp(1, 500) as usize)
                .unwrap_or(default_limit);

            let conn = Connection::open(&db)
                .map_err(|e| CruxErr::step_failed("handoff::pending", format!("open {db}: {e}")))?;

            let (where_clause, params): (&str, Vec<String>) = if let Some(proj) = &project_filter {
                (
                    "WHERE status <> 'done' AND project = ?1",
                    vec![proj.clone()],
                )
            } else {
                ("WHERE status <> 'done'", vec![])
            };

            // Count first, without a LIMIT, so `count` stays truthful when rows are capped.
            let count_sql = format!("SELECT COUNT(*) FROM items {where_clause}");
            let total: i64 = conn
                .query_row(&count_sql, params_from_iter(params.iter()), |r| r.get(0))
                .map_err(|e| CruxErr::step_failed("handoff::pending", format!("count: {e}")))?;

            // The project filter is bound as ?1 above, so the row query appends ?2 for LIMIT.
            let sql = format!(
                "SELECT project, id, name, priority, status FROM items {where_clause} \
                 ORDER BY priority, project, id LIMIT ?{n}",
                n = params.len() + 1
            );
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| CruxErr::step_failed("handoff::pending", format!("prepare: {e}")))?;

            let mut stmt_params = params.clone();
            stmt_params.push(limit.to_string());

            let items: Vec<Value> = stmt
                .query_map(params_from_iter(stmt_params.iter()), |row| {
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

            let returned = items.len();
            Ok(json!({
                "items": items,
                "count": total,
                "returned": returned,
                "truncated": (total as usize) > returned,
            }))
        }
    });
}
