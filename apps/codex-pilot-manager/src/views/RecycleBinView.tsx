import * as React from "react";
import { CheckCircle2, Download, History, RefreshCw, Trash2 } from "lucide-react";
import { callBackend } from "../backend";
import { Distribution, Metric } from "../components/primitives";
import type {
  ProviderCount,
  ProviderSyncSnapshot,
  RecycleBinBatchResponse,
  RecycleBinEntry,
  RecycleBinSnapshot,
  SessionZipExportResult,
  SessionZipImportMode,
  SessionZipImportResult,
  SessionZipInspectResult,
} from "../types";

export function RecycleBinView({
  recycleBin,
  onMessage,
  onProgress,
  onRefresh,
}: {
  recycleBin: RecycleBinSnapshot | null;
  onMessage: (message: string) => void;
  onProgress: (message: string) => void;
  onRefresh: () => void;
}) {
  const entries = recycleBin?.entries ?? [];
  const [selected, setSelected] = React.useState<string[]>([]);
  const [pendingAction, setPendingAction] = React.useState("");
  const [zipBusy, setZipBusy] = React.useState<"" | "export" | "inspect" | "import">("");
  const [zipInspect, setZipInspect] = React.useState<SessionZipInspectResult | null>(null);
  const [zipImportMode, setZipImportMode] = React.useState<SessionZipImportMode | "">("");
  const [zipOverwriteConfirm, setZipOverwriteConfirm] = React.useState(false);
  const [syncSnapshot, setSyncSnapshot] = React.useState<ProviderSyncSnapshot | null>(null);
  const [syncTarget, setSyncTarget] = React.useState("CodexPilot");
  const [customSyncTarget, setCustomSyncTarget] = React.useState("");
  const [syncInspecting, setSyncInspecting] = React.useState(false);
  const [syncBusy, setSyncBusy] = React.useState(false);
  const [syncConfirming, setSyncConfirming] = React.useState(false);
  const [deleteConfirming, setDeleteConfirming] = React.useState(false);
  const selectedEntries = entries.filter((entry) => selected.includes(entry.token));
  const recoverableSelected = selectedEntries.filter((entry) => entry.recoverable);
  const allSelected = entries.length > 0 && selected.length === entries.length;
  const selectedSyncTarget = syncTarget === "__custom" ? customSyncTarget.trim() : syncTarget;

  const refreshProviderSync = React.useCallback((target = "CodexPilot") => {
    return callBackend<ProviderSyncSnapshot>("provider_sync_snapshot", {
      request: { targetProvider: target || "CodexPilot" },
    })
      .then((snapshot) => {
        setSyncSnapshot(snapshot);
        setSyncConfirming(false);
        if (syncTarget !== "__custom") {
          setSyncTarget(snapshot.targetProvider || "CodexPilot");
        }
        return snapshot;
      });
  }, [syncTarget]);

  React.useEffect(() => {
    setSelected((current) => current.filter((token) => entries.some((entry) => entry.token === token)));
  }, [entries]);

  React.useEffect(() => {
    setDeleteConfirming(false);
  }, [selected.length, entries]);

  React.useEffect(() => {
    refreshProviderSync("CodexPilot")
      .catch((error) => onMessage(`检查对话同步失败：${String(error)}`));
  }, []);

  React.useEffect(() => {
    setSyncConfirming(false);
  }, [selectedSyncTarget]);

  React.useEffect(() => {
    setZipOverwriteConfirm(false);
  }, [zipImportMode, zipInspect?.zipPath]);

  const toggleAll = () => {
    setSelected(allSelected ? [] : entries.map((entry) => entry.token));
  };

  const toggleOne = (token: string) => {
    setSelected((current) =>
      current.includes(token)
        ? current.filter((item) => item !== token)
        : [...current, token]
    );
  };

  const restoreSelected = () => {
    if (!recoverableSelected.length || pendingAction) return;
    const tokens = recoverableSelected.map((entry) => entry.token);
    setPendingAction("restore");
    onProgress("正在恢复回收站会话");
    onMessage(`正在恢复 ${tokens.length} 条会话`);
    callBackend<RecycleBinBatchResponse>("restore_recycle_bin_entries", { request: { tokens } })
      .then((result) => {
        onMessage(result.message);
        const succeeded = new Set(result.succeededTokens);
        setSelected((current) => current.filter((token) => !succeeded.has(token)));
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPendingAction("");
        onProgress("");
      });
  };

  const deleteSelected = () => {
    if (!selected.length || pendingAction) return;
    if (!deleteConfirming) {
      setDeleteConfirming(true);
      onMessage(`再次点击"永久删除"以确认删除 ${selected.length} 条记录（不可恢复）`);
      return;
    }
    setDeleteConfirming(false);
    setPendingAction("delete");
    onProgress("正在永久删除回收站记录");
    onMessage(`正在永久删除 ${selected.length} 条记录`);
    callBackend<RecycleBinBatchResponse>("delete_recycle_bin_entries", { request: { tokens: selected } })
      .then((result) => {
        onMessage(result.message);
        const succeeded = new Set(result.succeededTokens);
        setSelected((current) => current.filter((token) => !succeeded.has(token)));
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPendingAction("");
        onProgress("");
      });
  };

  const exportSessionZip = () => {
    if (zipBusy) return;
    setZipBusy("export");
    onProgress("正在选择并导出对话 ZIP");
    onMessage("请选择 ZIP 保存位置");
    callBackend<string | null>("pick_session_zip_save_path")
      .then((path) => {
        if (!path) {
          onMessage("已取消导出 ZIP");
          return null;
        }
        onProgress("正在导出对话 ZIP");
        onMessage("正在导出当前本地对话库");
        return callBackend<SessionZipExportResult>("export_session_zip", {
          request: {
            zipPath: path,
          },
        });
      })
      .then((result) => {
        if (!result) return;
        onMessage(`导出完成：${result.zipPath}`);
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setZipBusy("");
        onProgress("");
      });
  };

  const pickAndInspectSessionZip = () => {
    if (zipBusy) return;
    setZipBusy("inspect");
    onProgress("正在选择并检查 ZIP");
    callBackend<string | null>("pick_session_zip_file")
      .then((path) => {
        if (!path) {
          onMessage("已取消选择 ZIP");
          return null;
        }
        return callBackend<SessionZipInspectResult>("inspect_session_zip", { zipPath: path });
      })
      .then((result) => {
        if (!result) return;
        setZipInspect(result);
        setZipImportMode("");
        onMessage(`已读取 ZIP：${result.zipPath}`);
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setZipBusy("");
        onProgress("");
      });
  };

  const runSessionZipImport = () => {
    if (!zipInspect || !zipImportMode || zipBusy) return;
    if (zipImportMode === "overwrite" && !zipOverwriteConfirm) {
      setZipOverwriteConfirm(true);
      onMessage("请再确认一次覆盖恢复。执行前会先创建本地安全备份 ZIP。");
      return;
    }
    setZipBusy("import");
    onProgress(zipImportMode === "merge" ? "正在合并导入 ZIP" : "正在覆盖恢复 ZIP");
    onMessage(zipImportMode === "merge" ? "正在合并导入对话 ZIP" : "正在覆盖恢复对话 ZIP");
    callBackend<SessionZipImportResult>("import_session_zip", {
      request: {
        zipPath: zipInspect.zipPath,
        mode: zipImportMode,
      },
    })
      .then((result) => {
        onMessage(result.message);
        setZipOverwriteConfirm(false);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setZipBusy("");
        onProgress("");
      });
  };

  const inspectProviderSync = () => {
    if (syncInspecting) return;
    const target = selectedSyncTarget || "CodexPilot";
    setSyncInspecting(true);
    onMessage(`正在检查对话同步：${target}`);
    refreshProviderSync(target)
      .then((snapshot) => onMessage(`检查完成：${providerSyncSummary(snapshot)}`))
      .catch((error) => onMessage(`检查对话同步失败：${String(error)}`))
      .finally(() => setSyncInspecting(false));
  };

  const runProviderSync = () => {
    const target = selectedSyncTarget || "CodexPilot";
    const pending = syncSnapshot
      ? syncSnapshot.rolloutRewriteNeeded + syncSnapshot.sqliteProviderRowsNeedingSync
      : 0;
    if (!syncConfirming) {
      setSyncConfirming(true);
      onMessage(`请再次确认同步：目标 ${target}，预计影响 ${pending} 项。`);
      return;
    }
    setSyncBusy(true);
    setSyncConfirming(false);
    onProgress("正在同步对话");
    onMessage(`正在同步对话：${target}`);
    callBackend<string>("sync_provider_sessions", { request: { targetProvider: target } })
      .then((message) => {
        onMessage(message);
        refreshProviderSync(target);
        onRefresh();
      })
      .catch((error) => onMessage(`同步对话失败：${String(error)}`))
      .finally(() => {
        setSyncBusy(false);
        onProgress("");
      });
  };

  const providerOptions = syncSnapshot?.availableProviders ?? ["CodexPilot"];
  const customTargetSelected = syncTarget === "__custom";
  const syncPending = syncSnapshot
    ? syncSnapshot.rolloutRewriteNeeded + syncSnapshot.sqliteProviderRowsNeedingSync
    : 0;
  const syncStatusTitle = !syncSnapshot
    ? "尚未检查历史会话"
    : syncPending > 0
      ? "历史会话需要同步"
      : "历史会话记录一致";
  const syncStatusDetail = !syncSnapshot
    ? "选择目标归属后预览影响，再决定是否同步。"
    : syncPending > 0
      ? `预计更新 ${syncSnapshot.rolloutRewriteNeeded} 个原始会话文件、${syncSnapshot.sqliteProviderRowsNeedingSync} 条本地索引记录。`
      : `${syncSnapshot.rolloutFiles} 个原始会话文件、${syncSnapshot.sqliteRows} 条本地索引记录已对齐。`;

  const recycleTooltip = (entry: RecycleBinEntry) => [
    `标题：${entry.title || "未命名会话"}`,
    `项目：${entry.projectCwd || "未知项目"}`,
    `会话 ID：${entry.sessionId || "-"}`,
    `备份：${entry.backupPath || "-"}`,
    `类型：${schemaLabel(entry.schema)}`,
    `状态：${entry.status || (entry.recoverable ? "可恢复" : "不可恢复")}`,
  ].join("\n");

  const mergeDisabled = !zipInspect || zipBusy === "import";
  const overwriteDisabled = !zipInspect || zipBusy === "import";
  const canRunZipImport =
    Boolean(zipInspect) &&
    Boolean(zipImportMode) &&
    (zipImportMode !== "overwrite" || zipOverwriteConfirm);

  const syncAction = (() => {
    if (syncBusy) {
      return (
        <button className="primary" disabled type="button">
          <RefreshCw size={16} />
          同步中
        </button>
      );
    }
    if (syncConfirming) {
      return (
        <div className="syncActionGroup">
          <button className="primary" onClick={runProviderSync} type="button">
            <RefreshCw size={16} />
            确认同步
          </button>
          <button className="secondary" onClick={() => setSyncConfirming(false)} type="button">
            取消
          </button>
        </div>
      );
    }
    if (syncPending <= 0) {
      return (
        <button className="secondary" disabled type="button">
          <CheckCircle2 size={16} />
          无需同步
        </button>
      );
    }
    return (
      <button className="primary" onClick={runProviderSync} type="button">
        <RefreshCw size={16} />
        同步
      </button>
    );
  })();

  return (
    <div className="sessionsLayout">
    <section className="panel recyclePanel">
      <div className="panelHeader">
        <div className="panelTitle">
          <Trash2 size={16} />
          <h2>回收站</h2>
        </div>
        <div className="buttonRow">
          <button className="secondary" onClick={onRefresh} type="button">刷新</button>
          <button
            className="secondary"
            disabled={!recoverableSelected.length || Boolean(pendingAction)}
            onClick={restoreSelected}
            type="button"
          >
            {pendingAction === "restore" ? "恢复中" : "恢复可恢复项"}
          </button>
          <button
            className="dangerButton"
            disabled={!selected.length || Boolean(pendingAction)}
            onClick={deleteSelected}
            type="button"
          >
            {pendingAction === "delete"
              ? "删除中"
              : deleteConfirming
                ? "再次点击确认"
                : "永久删除"}
          </button>
        </div>
      </div>
      <p className="formHint">
        共 {entries.length} 条记录，已选择 {selected.length} 条。永久删除只删除恢复备份，删除后不能恢复。
      </p>
      {entries.length ? (
        <div className="tableWrap">
          <table className="dataTable">
            <thead>
              <tr>
                <th>
                  <input
                    checked={allSelected}
                    onChange={toggleAll}
                    type="checkbox"
                    aria-label="选择全部回收站记录"
                  />
                </th>
                <th>标题</th>
                <th>来源</th>
                <th>最后活跃</th>
                <th>删除时间</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => {
                const title = entry.title || "未命名会话";
                const project = projectLabel(entry.projectCwd);
                const deletedAt = formatTimestamp(entry.deletedAt);
                const lastActiveAt = formatTimestamp(entry.lastActiveAt);
                return (
                  <tr key={entry.token} title={recycleTooltip(entry)}>
                    <td>
                      <input
                        checked={selected.includes(entry.token)}
                        onChange={() => toggleOne(entry.token)}
                        type="checkbox"
                        aria-label={`选择 ${title}`}
                      />
                    </td>
                    <td>
                      <span className="cellText strong" title={title}>{title}</span>
                    </td>
                    <td>
                      <span className="cellText" title={entry.projectCwd || project}>{project}</span>
                    </td>
                    <td>
                      <span className="cellText mono" title={lastActiveAt}>{lastActiveAt}</span>
                    </td>
                    <td>
                      <span className="cellText mono" title={deletedAt}>{deletedAt}</span>
                    </td>
                    <td>
                      <span className={`pill ${entry.recoverable ? "ok" : "warning"}`} title={entry.status}>
                        {entry.status || (entry.recoverable ? "可恢复" : "不可恢复")}
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="bodyText">暂无已删除会话。</p>
      )}
      <div className="sessionZipInlineBar">
        <div className="sessionZipInlineCopy">
          <div className="panelTitle compactTitle">
            <Download size={15} />
            <h3>备份与恢复</h3>
          </div>
          <p className="formHint">
            导出当前对话库为 ZIP，或导入已有 ZIP。导入前需要明确选择“合并导入”或“覆盖恢复”。
          </p>
        </div>
        <div className="buttonRow">
          <button className="secondary" disabled={Boolean(zipBusy)} onClick={pickAndInspectSessionZip} type="button">
            <Download size={16} />
            {zipBusy === "inspect" ? "检查中" : "导入 ZIP"}
          </button>
          <button className="primary" disabled={Boolean(zipBusy)} onClick={exportSessionZip} type="button">
            <Download size={16} />
            {zipBusy === "export" ? "导出中" : "导出 ZIP"}
          </button>
        </div>
      </div>
      {zipInspect ? (
        <div className="sessionZipPanel">
          <div className="zipInspectCard">
            <div className="zipInspectHeader">
              <strong>{zipInspect.zipPath}</strong>
              <span className="pill info">ZIP 已就绪</span>
            </div>
            <dl className="metricGrid overviewMetrics">
              <Metric label="导出时间" value={formatTimestampMs(zipInspect.manifest.exportedAtMs)} />
              <Metric label="sessions" value={zipInspect.entries.sessions ? `${zipInspect.manifest.counts.sessionFiles} 个文件` : "未包含"} />
              <Metric
                label="archived_sessions"
                value={zipInspect.entries.archivedSessions ? `${zipInspect.manifest.counts.archivedSessionFiles} 个文件` : "未包含"}
              />
              <Metric label="state_5.sqlite" value={zipInspect.entries.stateSqlite ? "已包含" : "未包含"} />
            </dl>
            <div className="zipModeGrid">
              <button
                className={`modeCard ${zipImportMode === "merge" ? "active" : ""}`}
                disabled={mergeDisabled}
                onClick={() => setZipImportMode("merge")}
                type="button"
              >
                <strong>合并导入</strong>
                <span>恢复 ZIP 里的会话文件；不会替换当前 `state_5.sqlite`。</span>
              </button>
              <button
                className={`modeCard dangerMode ${zipImportMode === "overwrite" ? "active" : ""}`}
                disabled={overwriteDisabled}
                onClick={() => setZipImportMode("overwrite")}
                type="button"
              >
                <strong>覆盖恢复</strong>
                <span>按 ZIP 覆盖当前本地会话目录；执行前会先创建一份本地安全备份 ZIP。</span>
              </button>
            </div>
            {zipImportMode === "merge" && (
              <div className="inlineNotice">
                <span className="statusDot ok" />
                <span>
                  当前选择“合并导入”。即使 ZIP 包含 `state_5.sqlite`，这次也不会导入数据库文件。
                </span>
              </div>
            )}
            {zipImportMode === "overwrite" && (
              <div className="overwriteConfirmCard">
                <div className="inlineNotice warning">
                  <span className="statusDot warning" />
                  <span>
                    当前选择“覆盖恢复”。它会替换当前本地对话目录，并在 ZIP 包含时替换 `state_5.sqlite`。
                  </span>
                </div>
                <label className="checkboxRow compactCheckbox">
                  <input
                    checked={zipOverwriteConfirm}
                    onChange={(event) => setZipOverwriteConfirm(event.target.checked)}
                    type="checkbox"
                  />
                  <span>我确认执行前先创建本地安全备份，然后再覆盖恢复。</span>
                </label>
              </div>
            )}
            <div className="buttonRow">
              <button
                className={zipImportMode === "overwrite" ? "dangerButton" : "primary"}
                disabled={!canRunZipImport || zipBusy === "import"}
                onClick={runSessionZipImport}
                type="button"
              >
                {zipBusy === "import"
                  ? zipImportMode === "overwrite"
                    ? "恢复中"
                    : "导入中"
                  : zipImportMode === "overwrite"
                    ? "执行覆盖恢复"
                    : "执行合并导入"}
              </button>
              <button
                className="secondary"
                disabled={Boolean(zipBusy)}
                onClick={() => {
                  setZipInspect(null);
                  setZipImportMode("");
                  setZipOverwriteConfirm(false);
                }}
                type="button"
              >
                清除当前 ZIP
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
    <section className="panel">
      <div className="panelHeader">
        <div className="panelTitle">
          <History size={16} />
          <h2>对话同步</h2>
        </div>
        <div className="buttonRow">
          <button className="secondary" disabled={syncInspecting} onClick={inspectProviderSync} type="button">
            <RefreshCw size={16} />
            {syncInspecting ? "检查中" : "预览影响"}
          </button>
        </div>
      </div>
      <div className="syncTool">
        <div className={`syncControls ${customTargetSelected ? "customMode" : ""}`}>
          <label>
            <span>目标 Provider</span>
            {customTargetSelected ? (
              <div className="syncFieldRow">
                <input
                  value={customSyncTarget}
                  onChange={(event) => setCustomSyncTarget(event.target.value)}
                  placeholder="provider-name"
                />
                <button
                  className="secondary"
                  onClick={() => {
                    setCustomSyncTarget("");
                    setSyncTarget(syncSnapshot?.currentProvider ?? providerOptions[0] ?? "CodexPilot");
                  }}
                  type="button"
                >
                  选择预设
                </button>
              </div>
            ) : (
              <div className="syncFieldRow">
                <select value={syncTarget} onChange={(event) => setSyncTarget(event.target.value)}>
                  {providerOptions.map((provider) => (
                    <option key={provider} value={provider}>{provider}</option>
                  ))}
                  <option value="__custom">自定义</option>
                </select>
              </div>
            )}
            <span className="fieldHint">将历史对话统一到这个 Provider；操作前可先预览影响。</span>
          </label>
        </div>
        <div className={`syncStatusCard ${syncPending > 0 ? "needsSync" : "ok"}`}>
          <div className="syncStatusMain">
            <div className="syncStatusCopy">
              <span className="syncStatusIcon">
                {syncPending > 0 ? <RefreshCw size={16} /> : <CheckCircle2 size={16} />}
              </span>
              <div>
                <strong>{syncStatusTitle}</strong>
                <p>{syncStatusDetail}</p>
              </div>
            </div>
            {syncAction}
          </div>
          <dl>
            <Metric label="目标归属" value={selectedSyncTarget || "CodexPilot"} />
            <Metric label="当前配置" value={syncSnapshot?.currentProvider ?? "-"} />
          </dl>
        </div>
        <details className="syncDetails">
          <summary>查看技术详情</summary>
          <div className="syncSummaryGrid">
            <Metric label="原始文件待改" value={`${syncSnapshot?.rolloutRewriteNeeded ?? 0}/${syncSnapshot?.rolloutFiles ?? 0}`} />
            <Metric label="本地索引待改" value={`${syncSnapshot?.sqliteProviderRowsNeedingSync ?? 0}/${syncSnapshot?.sqliteRows ?? 0}`} />
          </div>
          <div className="providerDistribution">
            <Distribution title="原始会话文件分布" items={syncSnapshot?.rolloutProviders ?? []} />
            <Distribution title="本地索引记录分布" items={syncSnapshot?.sqliteProviders ?? []} />
          </div>
        </details>
      </div>
    </section>
    </div>
  );
}

function formatTimestamp(value: number | null) {
  if (!value) return "-";
  return new Date(value * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatTimestampMs(value: number | null) {
  if (!value) return "-";
  return new Date(value).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function projectLabel(cwd: string | null) {
  if (!cwd) return "未知项目";
  const normalized = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] || cwd;
}

function schemaLabel(schema: string) {
  if (schema === "codex_threads") return "Codex 对话";
  if (schema === "generic_sessions") return "旧版会话";
  return schema || "未知";
}

function providerSyncSummary(snapshot: ProviderSyncSnapshot) {
  const pending = snapshot.rolloutRewriteNeeded + snapshot.sqliteProviderRowsNeedingSync;
  if (pending > 0) return `预计影响 ${pending} 项`;
  return "历史会话记录一致";
}
