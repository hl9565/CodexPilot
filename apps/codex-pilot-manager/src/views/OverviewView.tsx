import * as React from "react";
import { Activity, LogIn, Stethoscope, Terminal, Trash2 } from "lucide-react";
import { Metric } from "../components/primitives";
import type {
  BackendStatus,
  DiagnosticsSnapshot,
  LaunchSnapshot,
  ProviderSnapshot,
  RecycleBinSnapshot,
  ViewId,
} from "../types";

function canRunLaunchAction(launch: LaunchSnapshot | null): boolean {
  if (!launch) return false;
  return ["launch", "reinject", "restart", "running"].includes(launch.actionKind);
}

function backendStatusLabel(status: BackendStatus | null): string {
  if (!status) return "未连接";
  if (status.status === "running") return "已连接";
  return status.status || "未连接";
}

function runModeLabel(mode: ProviderSnapshot["mode"]): string {
  if (mode === "hybridApi") return "混合中转";
  if (mode === "api") return "传统中转";
  return "官方通道";
}

export function OverviewView({
  status,
  appVersion,
  launch,
  provider,
  recycleBin,
  diagnostics,
  onNavigate,
}: {
  status: BackendStatus | null;
  appVersion: string | null;
  launch: LaunchSnapshot | null;
  provider: ProviderSnapshot | null;
  recycleBin: RecycleBinSnapshot | null;
  diagnostics: DiagnosticsSnapshot | null;
  onNavigate: (view: ViewId) => void;
}) {
  const deletedCount = recycleBin?.entries.length ?? 0;
  const recoverableCount = recycleBin?.entries.filter((entry) => entry.recoverable).length ?? 0;
  const diagnosticsChecks = diagnostics?.checks ?? [];
  const failingChecks = diagnosticsChecks.filter((check) => !["ok", "pass", "passed"].includes(check.status)).length;
  const backendState = backendStatusLabel(status);
  const providerMode = runModeLabel(provider?.mode ?? "official");
  const displayVersion = appVersion ?? status?.version ?? "未知";
  const providerSummaryTitle = provider?.profile ?? "默认中转";
  const providerSummaryDetail = provider?.degraded
    ? "已检测到官方登录，但还没有可用恢复点，所以当前暂时按自动中转生效。"
    : provider?.statusMessage ?? "系统会根据官方登录和恢复点自动决定当前怎么生效。";

  return (
    <div className="taskStack">
      <section className="taskPanel primaryTask overviewLaunchTask">
        <div className="taskHeader">
          <div>
            <div className="panelTitle compactTitle titleLine">
              <span className="titleIcon">
                <Terminal size={16} />
              </span>
              <h2>启动与注入</h2>
              <span className={`statusPill ${canRunLaunchAction(launch) ? "ok" : "warning"}`}>
                <span className={`statusDot ${canRunLaunchAction(launch) ? "ok" : "warning"}`} />
                {launch?.actionLabel ?? "检查中"}
              </span>
            </div>
            <p className="taskSummary">主面板集中展示启动前最关键的状态和端口，详细路径与命令预览保留在启动设置页。</p>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="后端" value={backendState} />
          <Metric label="Codex 应用" value={launch?.appPath ? "已发现" : "未发现"} />
          <Metric label="调试端口" value={String(launch?.debugPort ?? "-")} />
          <Metric label="连接端口" value={String(launch?.helperPort ?? "-")} />
          <Metric label="版本" value={displayVersion} />
        </dl>
        <div className="taskFooter">
          <span className={`statusDot ${canRunLaunchAction(launch) ? "ok" : "warning"}`} />
          <span>{launch?.detail ?? "需要检查 Codex 应用路径或启动偏好"}</span>
          <button className="linkButton" onClick={() => onNavigate("launch")} type="button">查看启动设置</button>
        </div>
      </section>

      <section className="taskPanel providerTask">
        <div className="taskHeader">
          <div>
            <div className="panelTitle compactTitle titleLine">
              <span className="titleIcon">
                <LogIn size={16} />
              </span>
              <h2>当前配置档</h2>
            </div>
            <p className="taskSummary">系统会根据官方登录和恢复点自动决定当前怎么生效，详细维护在配置档页面。</p>
          </div>
          <button className="secondary" onClick={() => onNavigate("provider")} type="button">管理配置档</button>
        </div>
        <div className="providerOverviewBody">
          <div className={`providerSummaryCard ${provider?.degraded ? "warning" : ""}`}>
            <span className="providerSummaryLabel">当前使用</span>
            <strong>{providerSummaryTitle}</strong>
            <p>{providerSummaryDetail}</p>
          </div>
          <div className="providerSummaryGrid">
            <div className="summaryMetric">
              <span>官方登录</span>
              <strong>{provider?.authenticated ? "已检测" : "未检测"}</strong>
            </div>
            <div className="summaryMetric">
              <span>官方恢复点</span>
              <strong>{provider?.officialSnapshotAvailable ? "已准备" : "未准备"}</strong>
            </div>
            <div className="summaryMetric">
              <span>配置档</span>
              <strong>{providerSummaryTitle}</strong>
            </div>
            <div className="summaryMetric">
              <span>账号</span>
              <strong>{provider?.accountLabel ?? "未读取到账号信息"}</strong>
            </div>
          </div>
        </div>
      </section>

      <section className="taskPanel summaryTask">
        <div className="taskHeader summaryTaskHeader">
          <div className="panelTitle compactTitle">
            <span className="rowIcon"><Trash2 size={14} /></span>
            <h2>对话维护</h2>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="已删除" value={`${deletedCount} 条`} />
          <Metric label="可恢复" value={`${recoverableCount} 条`} />
        </dl>
        <button className="secondary summaryAction" onClick={() => onNavigate("sessions")} type="button">打开对话维护</button>
      </section>

      <section className="taskPanel summaryTask">
        <div className="taskHeader summaryTaskHeader">
          <div className="panelTitle compactTitle">
            <span className="rowIcon"><Stethoscope size={14} /></span>
            <h2>诊断摘要</h2>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="检查项" value={`${diagnosticsChecks.length} 项`} />
          <Metric label="需关注" value={`${failingChecks} 项`} />
        </dl>
        <button className="secondary summaryAction" onClick={() => onNavigate("diagnostics")} type="button">查看诊断</button>
      </section>
    </div>
  );
}
