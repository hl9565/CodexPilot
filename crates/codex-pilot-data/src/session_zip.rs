use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const PRODUCT_NAME: &str = "CodexPilot";
const MANIFEST_NAME: &str = "manifest.json";
const BACKUP_VERSION: u32 = 1;
const ZIP_BACKUP_KEEP_COUNT: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionZipManifest {
    pub version: u32,
    pub product: String,
    pub exported_at: String,
    pub exported_at_ms: u64,
    pub includes: SessionZipIncludes,
    pub counts: SessionZipCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionZipIncludes {
    pub sessions: bool,
    pub archived_sessions: bool,
    pub state_sqlite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionZipCounts {
    pub session_files: usize,
    pub archived_session_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionZipExportResult {
    pub zip_path: PathBuf,
    pub manifest: SessionZipManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionZipInspectResult {
    pub zip_path: PathBuf,
    pub manifest: SessionZipManifest,
    pub entries: SessionZipIncludes,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionZipImportMode {
    Merge,
    Overwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionZipImportResult {
    pub mode: SessionZipImportMode,
    pub manifest: SessionZipManifest,
    pub restored_session_files: usize,
    pub restored_archived_session_files: usize,
    pub restored_state_sqlite: bool,
    pub safety_backup_zip_path: Option<PathBuf>,
    pub message: String,
}

pub struct SessionZipService {
    codex_home: PathBuf,
}

impl SessionZipService {
    pub fn new(codex_home: PathBuf) -> Self {
        Self { codex_home }
    }

    pub fn export_current_state(&self) -> anyhow::Result<SessionZipExportResult> {
        fs::create_dir_all(self.backup_root())?;
        let zip_path = self.next_backup_zip_path("codex-sessions-backup")?;
        self.export_current_state_to_path(&zip_path)
    }

    pub fn export_current_state_to_path(
        &self,
        zip_path: &Path,
    ) -> anyhow::Result<SessionZipExportResult> {
        if let Some(parent) = zip_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(&zip_path)
            .with_context(|| format!("create backup zip {}", zip_path.display()))?;
        let mut writer = ZipWriter::new(file);
        let options = zip_file_options();

        let session_files = add_directory_to_zip(
            &mut writer,
            &self.codex_home.join("sessions"),
            "sessions",
            options,
        )?;
        let archived_session_files = add_directory_to_zip(
            &mut writer,
            &self.codex_home.join("archived_sessions"),
            "archived_sessions",
            options,
        )?;
        let state_sqlite = add_file_to_zip(
            &mut writer,
            &self.codex_home.join("state_5.sqlite"),
            "state_5.sqlite",
            options,
        )?;
        let manifest = SessionZipManifest {
            version: BACKUP_VERSION,
            product: PRODUCT_NAME.to_string(),
            exported_at: now_iso_like(),
            exported_at_ms: now_ms(),
            includes: SessionZipIncludes {
                sessions: self.codex_home.join("sessions").exists(),
                archived_sessions: self.codex_home.join("archived_sessions").exists(),
                state_sqlite,
            },
            counts: SessionZipCounts {
                session_files,
                archived_session_files,
            },
        };
        writer.start_file(MANIFEST_NAME, options)?;
        writer.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        writer.finish()?;
        if zip_path.starts_with(self.backup_root()) {
            prune_managed_backup_zips(self.backup_root())?;
        }
        Ok(SessionZipExportResult {
            zip_path: zip_path.to_path_buf(),
            manifest,
        })
    }

    pub fn inspect_zip(&self, zip_path: &Path) -> anyhow::Result<SessionZipInspectResult> {
        let mut archive = open_zip(zip_path)?;
        let manifest = read_manifest(&mut archive)?;
        let entries = scan_entries(&mut archive)?;
        ensure_any_included(&entries)?;
        Ok(SessionZipInspectResult {
            zip_path: zip_path.to_path_buf(),
            manifest,
            entries,
        })
    }

    pub fn import_zip(
        &self,
        zip_path: &Path,
        mode: SessionZipImportMode,
    ) -> anyhow::Result<SessionZipImportResult> {
        let inspection = self.inspect_zip(zip_path)?;
        let mut safety_backup_zip_path = None;
        if matches!(mode, SessionZipImportMode::Overwrite) {
            let backup = self.export_current_state()?;
            safety_backup_zip_path = Some(backup.zip_path);
        }

        let temp_dir = tempfile::tempdir()?;
        let mut archive = open_zip(zip_path)?;
        let extracted = extract_backup_archive(&mut archive, temp_dir.path())?;

        let (restored_session_files, restored_archived_session_files, restored_state_sqlite) =
            match mode {
                SessionZipImportMode::Merge => (
                    merge_directory(
                        extracted.sessions.as_deref(),
                        &self.codex_home.join("sessions"),
                    )?,
                    merge_directory(
                        extracted.archived_sessions.as_deref(),
                        &self.codex_home.join("archived_sessions"),
                    )?,
                    false,
                ),
                SessionZipImportMode::Overwrite => {
                    overwrite_directory(
                        extracted.sessions.as_deref(),
                        &self.codex_home.join("sessions"),
                    )?;
                    overwrite_directory(
                        extracted.archived_sessions.as_deref(),
                        &self.codex_home.join("archived_sessions"),
                    )?;
                    let restored_state = overwrite_file(
                        extracted.state_sqlite.as_deref(),
                        &self.codex_home.join("state_5.sqlite"),
                    )?;
                    (
                        count_files_if_present(&self.codex_home.join("sessions"))?,
                        count_files_if_present(&self.codex_home.join("archived_sessions"))?,
                        restored_state,
                    )
                }
            };

        let message = match mode {
            SessionZipImportMode::Merge => format!(
                "已合并导入 ZIP：sessions {} 个文件，archived_sessions {} 个文件；state_5.sqlite 保持不变。",
                restored_session_files, restored_archived_session_files
            ),
            SessionZipImportMode::Overwrite => format!(
                "已覆盖恢复 ZIP：sessions {} 个文件，archived_sessions {} 个文件，state_5.sqlite {}。已先创建安全备份{}。",
                restored_session_files,
                restored_archived_session_files,
                if restored_state_sqlite {
                    "已恢复"
                } else {
                    "未包含"
                },
                safety_backup_zip_path
                    .as_ref()
                    .map(|path| format!("：{}", path.display()))
                    .unwrap_or_default()
            ),
        };

        Ok(SessionZipImportResult {
            mode,
            manifest: inspection.manifest,
            restored_session_files,
            restored_archived_session_files,
            restored_state_sqlite,
            safety_backup_zip_path,
            message,
        })
    }

    fn backup_root(&self) -> PathBuf {
        self.codex_home.join("backups_state").join("session-zip")
    }

    fn next_backup_zip_path(&self, prefix: &str) -> anyhow::Result<PathBuf> {
        let mut candidate = self
            .backup_root()
            .join(format!("{prefix}-{}.zip", now_secs()));
        let mut suffix = 0usize;
        while candidate.exists() {
            suffix += 1;
            candidate = self
                .backup_root()
                .join(format!("{prefix}-{}-{suffix}.zip", now_secs()));
        }
        Ok(candidate)
    }
}

#[derive(Debug, Default)]
struct ExtractedBackup {
    sessions: Option<PathBuf>,
    archived_sessions: Option<PathBuf>,
    state_sqlite: Option<PathBuf>,
}

fn zip_file_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

fn open_zip(path: &Path) -> anyhow::Result<ZipArchive<fs::File>> {
    let file = fs::File::open(path).with_context(|| format!("open zip {}", path.display()))?;
    ZipArchive::new(file).with_context(|| format!("parse zip {}", path.display()))
}

fn read_manifest<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> anyhow::Result<SessionZipManifest> {
    let mut entry = archive
        .by_name(MANIFEST_NAME)
        .with_context(|| format!("{} missing", MANIFEST_NAME))?;
    let mut raw = String::new();
    entry.read_to_string(&mut raw)?;
    let manifest = serde_json::from_str::<SessionZipManifest>(&raw)
        .with_context(|| format!("parse {}", MANIFEST_NAME))?;
    Ok(manifest)
}

fn scan_entries<R: Read + Seek>(archive: &mut ZipArchive<R>) -> anyhow::Result<SessionZipIncludes> {
    let mut includes = SessionZipIncludes {
        sessions: false,
        archived_sessions: false,
        state_sqlite: false,
    };
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name();
        if name == "state_5.sqlite" {
            includes.state_sqlite = true;
        } else if name.starts_with("sessions/") {
            includes.sessions = true;
        } else if name.starts_with("archived_sessions/") {
            includes.archived_sessions = true;
        }
    }
    Ok(includes)
}

fn ensure_any_included(entries: &SessionZipIncludes) -> anyhow::Result<()> {
    if entries.sessions || entries.archived_sessions || entries.state_sqlite {
        Ok(())
    } else {
        bail!("ZIP 中未包含 sessions、archived_sessions 或 state_5.sqlite")
    }
}

fn add_directory_to_zip(
    writer: &mut ZipWriter<fs::File>,
    source_dir: &Path,
    zip_root: &str,
    options: SimpleFileOptions,
) -> anyhow::Result<usize> {
    if !source_dir.exists() {
        return Ok(0);
    }
    let mut count = 0usize;
    add_directory_recursive(
        writer, source_dir, source_dir, zip_root, options, &mut count,
    )?;
    Ok(count)
}

fn add_directory_recursive(
    writer: &mut ZipWriter<fs::File>,
    root: &Path,
    current: &Path,
    zip_root: &str,
    options: SimpleFileOptions,
    count: &mut usize,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(current)? {
        let path = entry?.path();
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("strip prefix {}", path.display()))?;
        let zip_name = format!(
            "{zip_root}/{}",
            relative.to_string_lossy().replace('\\', "/")
        );
        if path.is_dir() {
            add_directory_recursive(writer, root, &path, zip_root, options, count)?;
        } else if path.is_file() {
            writer.start_file(zip_name, options)?;
            let mut file = fs::File::open(&path)?;
            std::io::copy(&mut file, writer)?;
            *count += 1;
        }
    }
    Ok(())
}

fn add_file_to_zip(
    writer: &mut ZipWriter<fs::File>,
    source_file: &Path,
    zip_name: &str,
    options: SimpleFileOptions,
) -> anyhow::Result<bool> {
    if !source_file.exists() {
        return Ok(false);
    }
    writer.start_file(zip_name, options)?;
    let mut file = fs::File::open(source_file)?;
    std::io::copy(&mut file, writer)?;
    Ok(true)
}

fn extract_backup_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    destination: &Path,
) -> anyhow::Result<ExtractedBackup> {
    let mut extracted = ExtractedBackup::default();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if name == MANIFEST_NAME {
            continue;
        }
        let safe_relative = safe_relative_path(&name)?;
        let output_path = destination.join(&safe_relative);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut tmp = NamedTempFile::new_in(destination)?;
        std::io::copy(&mut entry, &mut tmp)?;
        tmp.persist(&output_path).map_err(|error| {
            anyhow::anyhow!("persist {}: {}", output_path.display(), error.error)
        })?;
        let normalized = safe_relative.to_string_lossy().replace('\\', "/");
        if normalized.starts_with("sessions/") {
            extracted.sessions = Some(destination.join("sessions"));
        } else if normalized.starts_with("archived_sessions/") {
            extracted.archived_sessions = Some(destination.join("archived_sessions"));
        } else if normalized == "state_5.sqlite" {
            extracted.state_sqlite = Some(output_path);
        }
    }
    Ok(extracted)
}

fn safe_relative_path(name: &str) -> anyhow::Result<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        bail!("ZIP 包含绝对路径：{name}");
    }
    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("ZIP 包含不安全路径：{name}")
            }
        }
    }
    if sanitized.as_os_str().is_empty() {
        bail!("ZIP 包含空路径");
    }
    Ok(sanitized)
}

fn merge_directory(source: Option<&Path>, destination: &Path) -> anyhow::Result<usize> {
    let Some(source) = source else {
        return Ok(0);
    };
    fs::create_dir_all(destination)?;
    copy_directory_contents(source, destination)
}

fn overwrite_directory(source: Option<&Path>, destination: &Path) -> anyhow::Result<()> {
    if destination.exists() {
        fs::remove_dir_all(destination)
            .with_context(|| format!("remove directory {}", destination.display()))?;
    }
    if let Some(source) = source {
        fs::create_dir_all(destination)?;
        copy_directory_contents(source, destination)?;
    }
    Ok(())
}

fn copy_directory_contents(source: &Path, destination: &Path) -> anyhow::Result<usize> {
    let mut count = 0usize;
    for entry in fs::read_dir(source)? {
        let path = entry?.path();
        let relative = path.strip_prefix(source)?;
        let target = destination.join(relative);
        if path.is_dir() {
            fs::create_dir_all(&target)?;
            count += copy_directory_contents(&path, &target)?;
        } else if path.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &target)
                .with_context(|| format!("copy {} -> {}", path.display(), target.display()))?;
            count += 1;
        }
    }
    Ok(count)
}

fn overwrite_file(source: Option<&Path>, destination: &Path) -> anyhow::Result<bool> {
    let Some(source) = source else {
        return Ok(false);
    };
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)
        .with_context(|| format!("copy {} -> {}", source.display(), destination.display()))?;
    Ok(true)
}

fn count_files_if_present(path: &Path) -> anyhow::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let mut count = 0usize;
    for entry in fs::read_dir(path)? {
        let entry_path = entry?.path();
        if entry_path.is_dir() {
            count += count_files_if_present(&entry_path)?;
        } else if entry_path.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

fn prune_managed_backup_zips(root: PathBuf) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut files = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("zip"))
        .collect::<Vec<_>>();
    files.sort();
    files.reverse();
    for path in files.into_iter().skip(ZIP_BACKUP_KEEP_COUNT) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn now_iso_like() -> String {
    now_ms().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_file(path: &Path, value: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, value).unwrap();
    }

    #[test]
    fn export_and_inspect_round_trip() {
        let temp = tempdir().unwrap();
        let home = temp.path().join(".codex");
        write_file(&home.join("sessions/2026/a.jsonl"), "a");
        write_file(&home.join("archived_sessions/2026/b.jsonl"), "b");
        write_file(&home.join("state_5.sqlite"), "sqlite");

        let service = SessionZipService::new(home.clone());
        let export = service.export_current_state().unwrap();
        assert!(export.zip_path.exists());
        assert_eq!(export.manifest.counts.session_files, 1);
        assert_eq!(export.manifest.counts.archived_session_files, 1);

        let inspect = service.inspect_zip(&export.zip_path).unwrap();
        assert!(inspect.entries.sessions);
        assert!(inspect.entries.archived_sessions);
        assert!(inspect.entries.state_sqlite);
    }

    #[test]
    fn inspect_rejects_unsafe_paths() {
        let temp = tempdir().unwrap();
        let zip_path = temp.path().join("unsafe.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = zip_file_options();
        writer.start_file("../evil.txt", options).unwrap();
        writer.write_all(b"evil").unwrap();
        writer.start_file(MANIFEST_NAME, options).unwrap();
        writer
            .write_all(
                serde_json::to_string(&SessionZipManifest {
                    version: 1,
                    product: PRODUCT_NAME.to_string(),
                    exported_at: "1".to_string(),
                    exported_at_ms: 1,
                    includes: SessionZipIncludes {
                        sessions: true,
                        archived_sessions: false,
                        state_sqlite: false,
                    },
                    counts: SessionZipCounts {
                        session_files: 1,
                        archived_session_files: 0,
                    },
                })
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        writer.finish().unwrap();

        let service = SessionZipService::new(temp.path().join(".codex"));
        let result = service.import_zip(&zip_path, SessionZipImportMode::Merge);
        assert!(result.is_err());
    }

    #[test]
    fn merge_does_not_replace_state_sqlite() {
        let temp = tempdir().unwrap();
        let home = temp.path().join(".codex");
        write_file(&home.join("state_5.sqlite"), "local");
        write_file(&home.join("sessions/existing.jsonl"), "old");
        let service = SessionZipService::new(home.clone());

        let source_home = temp.path().join("source");
        write_file(&source_home.join("state_5.sqlite"), "archive");
        write_file(&source_home.join("sessions/new.jsonl"), "new");
        let source_service = SessionZipService::new(source_home);
        let export = source_service.export_current_state().unwrap();

        let result = service
            .import_zip(&export.zip_path, SessionZipImportMode::Merge)
            .unwrap();
        assert_eq!(result.restored_session_files, 1);
        assert!(!result.restored_state_sqlite);
        assert_eq!(
            fs::read_to_string(home.join("state_5.sqlite")).unwrap(),
            "local"
        );
        assert_eq!(
            fs::read_to_string(home.join("sessions/new.jsonl")).unwrap(),
            "new"
        );
    }

    #[test]
    fn overwrite_creates_safety_backup_before_restore() {
        let temp = tempdir().unwrap();
        let home = temp.path().join(".codex");
        write_file(&home.join("state_5.sqlite"), "local");
        write_file(&home.join("sessions/local.jsonl"), "local");
        let service = SessionZipService::new(home.clone());

        let source_home = temp.path().join("source");
        write_file(&source_home.join("state_5.sqlite"), "archive");
        write_file(&source_home.join("sessions/remote.jsonl"), "remote");
        let export = SessionZipService::new(source_home)
            .export_current_state()
            .unwrap();

        let result = service
            .import_zip(&export.zip_path, SessionZipImportMode::Overwrite)
            .unwrap();
        assert!(result.safety_backup_zip_path.as_ref().unwrap().exists());
        assert!(result.restored_state_sqlite);
        assert_eq!(
            fs::read_to_string(home.join("state_5.sqlite")).unwrap(),
            "archive"
        );
        assert!(!home.join("sessions/local.jsonl").exists());
        assert_eq!(
            fs::read_to_string(home.join("sessions/remote.jsonl")).unwrap(),
            "remote"
        );
    }
}
