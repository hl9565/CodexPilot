use crate::provider_sync::filesystem::{
    dirs_home, normalize_target_provider, read_current_provider,
};
use crate::provider_sync::models::{ProviderSyncInspection, provider_counts};
use crate::provider_sync::session_changes::{
    collect_session_changes, rollout_provider_from_first_line,
};
use crate::provider_sync::sqlite::{
    count_sqlite_provider_rows_needing_sync, count_sqlite_rows, count_sqlite_updates,
    sqlite_provider_counts,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub fn inspect_provider_sync(codex_home: Option<&Path>) -> anyhow::Result<ProviderSyncInspection> {
    let home = codex_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs_home().join(".codex"));
    inspect_provider_sync_with_target(Some(&home), None)
}

pub fn inspect_provider_sync_with_target(
    codex_home: Option<&Path>,
    target_provider: Option<&str>,
) -> anyhow::Result<ProviderSyncInspection> {
    let home = codex_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs_home().join(".codex"));
    let target_provider = normalize_target_provider(
        target_provider
            .map(ToString::to_string)
            .unwrap_or_else(|| read_current_provider(&home.join("config.toml"))),
    );
    let changes = collect_session_changes(&home, &target_provider)?;
    let thread_ids_with_user_events = changes
        .iter()
        .filter(|change| change.has_user_event)
        .filter_map(|change| change.thread_id.clone())
        .collect::<HashSet<_>>();
    let cwd_by_thread_id = changes
        .iter()
        .filter_map(|change| Some((change.thread_id.clone()?, change.cwd.clone()?)))
        .collect::<HashMap<_, _>>();
    let sqlite_path = home.join("state_5.sqlite");
    let sqlite_total_updates_needed = count_sqlite_updates(
        &sqlite_path,
        &target_provider,
        &thread_ids_with_user_events,
        &cwd_by_thread_id,
    )?;

    let sqlite_provider_rows_needing_sync =
        count_sqlite_provider_rows_needing_sync(&sqlite_path, &target_provider)?;

    Ok(ProviderSyncInspection {
        target_provider,
        rollout_files: changes.len(),
        rollout_rewrite_needed: changes
            .iter()
            .filter(|change| change.rewrite_needed)
            .count(),
        sqlite_rows: count_sqlite_rows(&sqlite_path)?,
        sqlite_provider_rows_needing_sync,
        sqlite_total_updates_needed,
        rollout_providers: provider_counts(
            changes
                .iter()
                .filter_map(|change| rollout_provider_from_first_line(&change.original_first_line)),
        ),
        sqlite_providers: sqlite_provider_counts(&sqlite_path)?,
    })
}
