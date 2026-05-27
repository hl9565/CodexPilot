use crate::provider_sync::models::{ProviderCount, ProviderDriftDetail};
use crate::provider_sync::session_changes::rollout_provider_from_path;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn table_columns(db: &Connection, table: &str) -> anyhow::Result<HashSet<String>> {
    let mut stmt = db.prepare(&format!(
        "PRAGMA table_info(\"{}\")",
        table.replace('"', "\"\"")
    ))?;
    Ok(stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<HashSet<_>>>()?)
}

pub(super) fn count_sqlite_updates(
    path: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> anyhow::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let db = Connection::open(path)?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(0);
    }
    let mut total: usize = db.query_row(
        "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
        [target_provider],
        |row| row.get::<_, i64>(0),
    )? as usize;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            total += db.query_row(
                "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
                [thread_id],
                |row| row.get::<_, i64>(0),
            )? as usize;
        }
    }
    if columns.contains("cwd") {
        for (thread_id, cwd) in cwd_by_thread_id {
            total += db.query_row(
                "SELECT COUNT(*) FROM threads WHERE id = ?1 AND COALESCE(cwd, '') <> ?2",
                (thread_id, cwd),
                |row| row.get::<_, i64>(0),
            )? as usize;
        }
    }
    Ok(total)
}

pub(super) fn count_sqlite_rows(path: &Path) -> anyhow::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let db = Connection::open(path)?;
    if !table_columns(&db, "threads").is_ok() {
        return Ok(0);
    }
    Ok(db.query_row("SELECT COUNT(*) FROM threads", [], |row| {
        row.get::<_, i64>(0)
    })? as usize)
}

pub(super) fn count_sqlite_provider_rows_needing_sync(
    path: &Path,
    target_provider: &str,
) -> anyhow::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let db = Connection::open(path)?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(0);
    }
    Ok(db.query_row(
        "SELECT COUNT(*) FROM threads WHERE COALESCE(model_provider, '') <> ?1",
        [target_provider],
        |row| row.get::<_, i64>(0),
    )? as usize)
}

pub(super) fn sqlite_provider_counts(path: &Path) -> anyhow::Result<Vec<ProviderCount>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let db = Connection::open(path)?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(Vec::new());
    }
    let mut stmt = db.prepare(
        "SELECT COALESCE(model_provider, ''), COUNT(*) FROM threads GROUP BY COALESCE(model_provider, '')",
    )?;
    let mut items = stmt
        .query_map([], |row| {
            Ok(ProviderCount {
                provider: row.get::<_, String>(0)?,
                count: row.get::<_, i64>(1)? as usize,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.provider.cmp(&right.provider))
    });
    Ok(items)
}

pub(super) fn sqlite_provider_drift_details(
    path: &Path,
    target_provider: &str,
) -> anyhow::Result<Vec<ProviderDriftDetail>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let db = Connection::open(path)?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(Vec::new());
    }
    let has_thread_source = columns.contains("thread_source");
    let has_updated_at_ms = columns.contains("updated_at_ms");
    let select_thread_source = if has_thread_source {
        "COALESCE(thread_source, '')"
    } else {
        "''"
    };
    let select_updated_at_ms = if has_updated_at_ms {
        "updated_at_ms"
    } else {
        "NULL"
    };
    let order_updated_at_ms = if has_updated_at_ms {
        "updated_at_ms DESC,"
    } else {
        ""
    };
    let sql = format!(
        "SELECT id, COALESCE(title, ''), COALESCE(source, ''), {select_thread_source}, COALESCE(model_provider, ''), {select_updated_at_ms}, rollout_path \
         FROM threads WHERE COALESCE(model_provider, '') <> ?1 ORDER BY {order_updated_at_ms} id LIMIT 50"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map([target_provider], |row| {
        let rollout_path: String = row.get(6)?;
        Ok(ProviderDriftDetail {
            id: row.get(0)?,
            title: row.get(1)?,
            source: row.get(2)?,
            thread_source: row.get(3)?,
            sqlite_provider: row.get(4)?,
            rollout_provider: rollout_provider_from_path(Path::new(&rollout_path)),
            updated_at_ms: row.get(5)?,
            rollout_path,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub(super) fn apply_sqlite_update(
    path: &Path,
    target_provider: &str,
    user_event_thread_ids: &HashSet<String>,
    cwd_by_thread_id: &HashMap<String, String>,
) -> anyhow::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let mut db = Connection::open(path)?;
    let columns = table_columns(&db, "threads")?;
    if !columns.contains("model_provider") {
        return Ok(0);
    }
    let tx = db.transaction()?;
    let provider_rows = tx.execute(
        "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
        [target_provider],
    )?;
    if columns.contains("has_user_event") {
        for thread_id in user_event_thread_ids {
            tx.execute(
                "UPDATE threads SET has_user_event = 1 WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
                [thread_id],
            )?;
        }
    }
    if columns.contains("cwd") {
        for (thread_id, cwd) in cwd_by_thread_id {
            tx.execute(
                "UPDATE threads SET cwd = ?1 WHERE id = ?2 AND COALESCE(cwd, '') <> ?1",
                (cwd, thread_id),
            )?;
        }
    }
    tx.commit()?;
    Ok(provider_rows)
}
