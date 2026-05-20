import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  BadgeCheck,
  Bot,
  CheckCircle2,
  Clipboard,
  Download,
  Gauge,
  History,
  Trash2,
  LogIn,
  Play,
  RefreshCw,
  Settings,
  Stethoscope,
  Terminal,
  Eye,
  EyeOff,
  Network,
  RotateCcw,
  Plus,
} from "lucide-react";
import "./styles.css";

type BackendStatus = {
  status: string;
  version: string;
};

type LaunchSnapshot = {
  appPath: string | null;
  requestedAppPath: string;
  debugPort: number;
  helperPort: number;
  ready: boolean;
  state: string;
  actionKind: string;
  actionLabel: string;
  helperReachable: boolean;
  debugReachable: boolean;
  codexRunning: boolean;
  detail: string;
  commandPreview: string[];
};

type ProviderSnapshot = {
  activeProvider: string;
  mode: RunMode;
  profile: string;
  source: string;
  authPath: string;
  configured: boolean;
  authenticated: boolean;
  accountLabel: string | null;
  profiles: ProviderProfile[];
  activeProfileId: string;
};

type ProviderProfile = {
  id: string;
  name: string;
  baseUrl: string;
  bearerToken: string;
  mode: ProviderProfileMode;
};

type RunMode = "official" | "hybridApi" | "api";
type ProviderProfileMode = "hybridApi" | "api";

type ProviderProfileSaveResponse = {
  id: string;
  message: string;
};

type DiagnosticCheck = {
  name: string;
  status: string;
  detail: string;
};

type DiagnosticsSnapshot = {
  checks: DiagnosticCheck[];
  logs: string[];
};

type RecycleBinEntry = {
  token: string;
  sessionId: string;
  title: string | null;
  schema: string;
  dbPath: string;
  backupPath: string;
  deletedAt: number | null;
  recoverable: boolean;
  status: string;
};

type RecycleBinSnapshot = {
  entries: RecycleBinEntry[];
};

type ViewId = "overview" | "launch" | "provider" | "sessions" | "diagnostics";

const views: Array<{ id: ViewId; label: string; icon: React.ElementType }> = [
  { id: "overview", label: "总览", icon: Activity },
  { id: "launch", label: "启动", icon: Terminal },
  { id: "provider", label: "模型通道", icon: LogIn },
  { id: "sessions", label: "回收站", icon: History },
  { id: "diagnostics", label: "诊断", icon: Stethoscope },
];

function canRunLaunchAction(launch: LaunchSnapshot | null): boolean {
  if (!launch) return false;
  return ["launch", "reinject", "restart", "running"].includes(launch.actionKind);
}

function backendStatusLabel(status: BackendStatus | null): string {
  if (!status) return "未连接";
  if (status.status === "running") return "已连接";
  return status.status || "未连接";
}

function App() {
  const [activeView, setActiveView] = React.useState<ViewId>("overview");
  const [status, setStatus] = React.useState<BackendStatus | null>(null);
  const [appVersion, setAppVersion] = React.useState<string | null>(null);
  const [launch, setLaunch] = React.useState<LaunchSnapshot | null>(null);
  const [provider, setProvider] = React.useState<ProviderSnapshot | null>(null);
  const [recycleBin, setRecycleBin] = React.useState<RecycleBinSnapshot | null>(null);
  const [diagnostics, setDiagnostics] = React.useState<DiagnosticsSnapshot | null>(null);
  const [message, setMessage] = React.useState("就绪");
  const [toast, setToast] = React.useState("");
  const [progressMessage, setProgressMessage] = React.useState("");
  const [launching, setLaunching] = React.useState(false);

  const notify = React.useCallback((value: string) => {
    setMessage(value);
    setToast(value);
  }, []);

  React.useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(""), 3200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const refresh = React.useCallback((silent = false) => {
    if (!silent) notify("正在刷新");
    Promise.all([
      invoke<BackendStatus | null>("backend_status")
        .then(setStatus)
        .catch(() => setStatus(null)),
      invoke<LaunchSnapshot>("launch_snapshot")
        .then(setLaunch)
        .catch(() => setLaunch(null)),
      invoke<ProviderSnapshot>("provider_snapshot")
        .then(setProvider)
        .catch(() => setProvider(null)),
      invoke<RecycleBinSnapshot>("recycle_bin_snapshot")
        .then(setRecycleBin)
        .catch(() => setRecycleBin(null)),
      invoke<DiagnosticsSnapshot>("diagnostics_snapshot")
        .then(setDiagnostics)
        .catch(() => setDiagnostics(null)),
    ]).finally(() => {
      if (!silent) notify("已更新");
    });
  }, [notify]);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  React.useEffect(() => {
    const refreshWhenVisible = () => {
      if (document.visibilityState === "visible") {
        refresh(true);
      }
    };
    window.addEventListener("focus", refreshWhenVisible);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    return () => {
      window.removeEventListener("focus", refreshWhenVisible);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [refresh]);

  React.useEffect(() => {
    invoke<string>("app_version")
      .then(setAppVersion)
      .catch(() => setAppVersion(null));
  }, []);

  const handleLaunch = () => {
    if (launching) return;
    const actionKind = launch?.actionKind ?? "launch";
    if (actionKind === "unavailable") {
      notify("需要检查 Codex 应用路径或启动偏好");
      return;
    }
    if (actionKind === "restart") {
      const confirmed = window.confirm(
        "当前 Codex 不是通过 CodexPilot 启动，无法直接注入。重启会关闭 Codex 当前窗口，未保存输入可能丢失。是否继续？",
      );
      if (!confirmed) return;
    }
    const command =
      actionKind === "reinject"
        ? "reinject_codex"
        : actionKind === "restart"
          ? "restart_codex_and_inject"
          : "launch_codex";
    const progress =
      actionKind === "reinject"
        ? "正在重新注入 CodexPilot"
        : actionKind === "restart"
          ? "正在重启 Codex"
          : "正在启动 Codex";
    setLaunching(true);
    setProgressMessage(progress);
    notify(progress);
    invoke<string>(command)
      .then((value) => {
        notify(value);
        refresh();
      })
      .catch((error) => notify(String(error)))
      .finally(() => {
        setLaunching(false);
        setProgressMessage("");
      });
  };

  return (
    <main className="shell">
      <aside className="sidebar">
        <div className="brand">
          <Bot size={20} />
          <span>CodexPilot</span>
        </div>
        <nav className="navList" aria-label="管理视图">
          {views.map((view) => {
            const Icon = view.icon;
            return (
              <button
                className={`nav ${activeView === view.id ? "active" : ""}`}
                key={view.id}
                onClick={() => setActiveView(view.id)}
                type="button"
              >
                <Icon size={16} />
                {view.label}
              </button>
            );
          })}
        </nav>
      </aside>

      <section className="content">
        <header className="pageHeader">
          <div>
            <p className="eyebrow">管理工具</p>
            <h1>{views.find((view) => view.id === activeView)?.label}</h1>
          </div>
          <div className="headerActions">
            <span className="message">{message}</span>
            <button className="secondary iconButton" onClick={() => refresh()} title="刷新" type="button">
              <RefreshCw size={16} />
            </button>
            {(activeView === "overview" || activeView === "launch") && (
              <button className="primary" disabled={launching || !canRunLaunchAction(launch)} onClick={handleLaunch} type="button">
                {launch?.actionKind === "reinject" || launch?.actionKind === "restart" ? <RotateCcw size={16} /> : <Play size={16} />}
                {launching ? "处理中" : launch?.actionLabel ?? "启动 Codex"}
              </button>
            )}
          </div>
        </header>

        {activeView === "overview" && (
          <OverviewView
            status={status}
            appVersion={appVersion}
            launch={launch}
            provider={provider}
            recycleBin={recycleBin}
            diagnostics={diagnostics}
            onNavigate={setActiveView}
          />
        )}
        {activeView === "launch" && <LaunchView status={status} launch={launch} onRefresh={refresh} />}
        {activeView === "provider" && (
          <ProviderView
            provider={provider}
            onMessage={notify}
            onProgress={setProgressMessage}
            onRefresh={refresh}
          />
        )}
        {activeView === "sessions" && (
          <RecycleBinView
            recycleBin={recycleBin}
            onMessage={notify}
            onProgress={setProgressMessage}
            onRefresh={refresh}
          />
        )}
        {activeView === "diagnostics" && (
          <DiagnosticsView
            diagnostics={diagnostics}
            onRefresh={refresh}
            onMessage={notify}
            onProgress={setProgressMessage}
          />
        )}
      </section>
      {progressMessage && <ProgressDialog message={progressMessage} />}
      {toast && (
        <div className="appToast" role="status">
          {toast}
        </div>
      )}
    </main>
  );
}

function OverviewView({
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

  return (
    <div className="taskStack">
      <section className="taskPanel primaryTask">
        <div className="taskHeader">
          <div className="panelTitle compactTitle">
            <Terminal size={16} />
            <h2>启动就绪</h2>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="后端" value={backendState} />
          <Metric label="Codex 应用" value={launch?.appPath ? "已发现" : "未发现"} />
          <Metric label="调试端口" value={String(launch?.debugPort ?? "-")} />
          <Metric label="后端端口" value={String(launch?.helperPort ?? "-")} />
          <Metric label="版本" value={displayVersion} />
        </dl>
        <div className="taskFooter">
          <span className={`statusDot ${canRunLaunchAction(launch) ? "ok" : "warning"}`} />
          <span>{launch?.detail ?? "需要检查 Codex 应用路径或启动偏好"}</span>
          <button className="linkButton" onClick={() => onNavigate("launch")} type="button">查看启动设置</button>
        </div>
      </section>

      <section className="taskPanel">
        <div className="taskHeader">
          <div className="panelTitle compactTitle">
            <LogIn size={16} />
            <h2>模型通道</h2>
          </div>
          <button className="secondary" onClick={() => onNavigate("provider")} type="button">选择通道</button>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="通道" value={providerMode} />
          <Metric label="官方登录" value={provider?.authenticated ? "已检测" : "未检测"} />
          <Metric label="配置档" value={provider?.profile ?? "默认"} />
        </dl>
        <div className="accountLine">
          <span className={`statusDot ${provider?.authenticated ? "ok" : "warning"}`} />
          <span>登录账号</span>
          <strong>{provider?.accountLabel ?? "未读取到账号信息"}</strong>
        </div>
      </section>

      <section className="taskPanel">
        <div className="taskHeader">
          <div className="panelTitle compactTitle">
            <Trash2 size={16} />
            <h2>回收站</h2>
          </div>
          <button className="secondary" onClick={() => onNavigate("sessions")} type="button">打开回收站</button>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="已删除" value={`${deletedCount} 条`} />
          <Metric label="可恢复" value={`${recoverableCount} 条`} />
        </dl>
      </section>

      <section className="taskPanel">
        <div className="taskHeader">
          <div className="panelTitle compactTitle">
            <Stethoscope size={16} />
            <h2>诊断摘要</h2>
          </div>
          <button className="secondary" onClick={() => onNavigate("diagnostics")} type="button">查看诊断</button>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="检查项" value={`${diagnosticsChecks.length} 项`} />
          <Metric label="需关注" value={`${failingChecks} 项`} />
        </dl>
      </section>
    </div>
  );
}

function LaunchView({
  status,
  launch,
  onRefresh,
}: {
  status: BackendStatus | null;
  launch: LaunchSnapshot | null;
  onRefresh: () => void;
}) {
  const [appPath, setAppPath] = React.useState("");
  const [debugPort, setDebugPort] = React.useState("9688");
  const [helperPort, setHelperPort] = React.useState("58888");
  const [saveMessage, setSaveMessage] = React.useState("");
  const backendState = backendStatusLabel(status);
  const connectionState = launch?.debugReachable ? "可直接注入" : launch?.codexRunning ? "需要重启注入" : "可启动";

  React.useEffect(() => {
    if (!launch) return;
    setAppPath(launch.requestedAppPath || launch.appPath || "");
    setDebugPort(String(launch.debugPort));
    setHelperPort(String(launch.helperPort));
  }, [launch]);

  const savePreferences = () => {
    const debug = Number(debugPort);
    const helper = Number(helperPort);
    if (!Number.isInteger(debug) || debug <= 0 || debug > 65535) {
      setSaveMessage("调试端口必须是 1 到 65535 的整数");
      return;
    }
    if (!Number.isInteger(helper) || helper <= 0 || helper > 65535) {
      setSaveMessage("后端端口必须是 1 到 65535 的整数");
      return;
    }
    if (debug === helper) {
      setSaveMessage("调试端口和后端端口不能相同");
      return;
    }
    invoke<string>("save_launch_preferences", {
      request: {
        appPath,
        debugPort: debug,
        helperPort: helper,
      },
    })
      .then((message) => {
        setSaveMessage(message);
        onRefresh();
      })
      .catch((error) => setSaveMessage(String(error)));
  };

  return (
    <div className="taskStack">
      <section className="taskPanel primaryTask">
        <div className="taskHeader">
          <div className="panelTitle compactTitle">
            <Gauge size={16} />
            <h2>启动状态</h2>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="后端" value={backendState} />
          <Metric label="Codex" value={launch?.codexRunning ? "已运行" : "未检测"} />
          <Metric label="连接方式" value={connectionState} />
          <Metric label="调试端口" value={String(launch?.debugPort ?? "-")} />
        </dl>
      </section>

      <section className="panel">
        <div className="panelTitle">
          <CheckCircle2 size={16} />
          <h2>运行环境</h2>
        </div>
        <div className="rows">
          <Row label="Codex 应用" value={launch?.appPath ?? "未发现"} />
          <Row label="偏好路径" value={launch?.requestedAppPath || "自动探测"} />
          <Row label="调试端口" value={String(launch?.debugPort ?? "-")} />
          <Row label="后端端口" value={String(launch?.helperPort ?? "-")} />
          <Row label="调试端口状态" value={launch?.debugReachable ? "可连接" : "未连接"} />
          <Row label="后端端口状态" value={launch?.helperReachable ? "可连接" : "未连接"} />
        </div>
        <pre className="commandBlock">
          {launch?.commandPreview.length ? launch.commandPreview.join(" ") : "暂无启动命令"}
        </pre>
      </section>

      <section className="panel">
        <div className="panelTitle">
          <Settings size={16} />
          <h2>启动偏好</h2>
        </div>
        <div className="formStack">
          <label>
            <span>Codex 应用路径</span>
            <input
              value={appPath}
              onChange={(event) => setAppPath(event.target.value)}
              placeholder="/Applications/Codex.app"
            />
          </label>
          <label>
            <span>调试端口</span>
            <input
              inputMode="numeric"
              value={debugPort}
              onChange={(event) => setDebugPort(event.target.value)}
              placeholder="9688"
            />
          </label>
          <label>
            <span>后端端口</span>
            <input
              inputMode="numeric"
              value={helperPort}
              onChange={(event) => setHelperPort(event.target.value)}
              placeholder="58888"
            />
          </label>
          <div className="buttonRow">
            <button className="primary" onClick={savePreferences} type="button">保存偏好</button>
            <button className="secondary" onClick={() => setAppPath("")} type="button">使用自动探测</button>
          </div>
          {saveMessage && <p className="formMessage">{saveMessage}</p>}
        </div>
      </section>
    </div>
  );
}

function ProviderView({
  provider,
  onMessage,
  onProgress,
  onRefresh,
}: {
  provider: ProviderSnapshot | null;
  onMessage: (message: string) => void;
  onProgress: (message: string) => void;
  onRefresh: () => void;
}) {
  const profiles = provider?.profiles ?? [];
  const activeProfileId = provider?.activeProfileId || profiles[0]?.id || "";
  const activeProfile = profiles.find((profile) => profile.id === activeProfileId) ?? profiles[0] ?? null;
  const snapshotMode = provider?.mode ?? "official";
  const [selectedChannel, setSelectedChannel] = React.useState<"official" | "hybridApi">(
    snapshotMode === "official" ? "official" : "hybridApi"
  );
  const customChannelSelected = selectedChannel === "hybridApi";
  const [editingId, setEditingId] = React.useState("");
  const [profileName, setProfileName] = React.useState("");
  const [baseUrl, setBaseUrl] = React.useState("");
  const [bearerToken, setBearerToken] = React.useState("");
  const [showToken, setShowToken] = React.useState(false);
  const [isCreatingProfile, setIsCreatingProfile] = React.useState(false);
  const [pendingAction, setPendingAction] = React.useState("");
  const currentMode = snapshotMode;
  const visibleProfiles: ProviderProfile[] = isCreatingProfile
    ? [{ id: "", name: profileName || "新中转", baseUrl, bearerToken, mode: "hybridApi" }]
    : profiles;

  React.useEffect(() => {
    setSelectedChannel(snapshotMode === "official" ? "official" : "hybridApi");
  }, [snapshotMode]);

  React.useEffect(() => {
    if (!activeProfile || isCreatingProfile) return;
    setEditingId(activeProfile.id);
    setProfileName(activeProfile.name);
    setBaseUrl(activeProfile.baseUrl);
    setBearerToken(activeProfile.bearerToken);
  }, [activeProfile?.id, isCreatingProfile]);

  const saveMixedRelay = () => {
    if (pendingAction) return;
    if (!profileName.trim() || !baseUrl.trim() || !bearerToken.trim()) {
      onMessage("名称、Base URL 和 Key 不能为空");
      return;
    }
    setPendingAction("save");
    onProgress("正在保存混合中转");
    onMessage("正在保存混合中转");
    invoke<ProviderProfileSaveResponse>("save_provider_profile", {
      request: {
        id: editingId || null,
        name: profileName,
        baseUrl,
        bearerToken,
        mode: "hybridApi",
        activate: true,
      },
    })
      .then((saveResult) => {
        setEditingId(saveResult.id);
        setIsCreatingProfile(false);
        onMessage(saveResult.message);
        return invoke<string>("apply_provider", {
          request: {
            profileId: saveResult.id,
            mode: "hybridApi",
          },
        });
      })
      .then((applyMessage) => {
        if (!applyMessage) return;
        onMessage(applyMessage);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPendingAction("");
        onProgress("");
      });
  };

  const newProfile = () => {
    setEditingId("");
    setProfileName("新中转");
    setBaseUrl("");
    setBearerToken("");
    setShowToken(false);
    setIsCreatingProfile(true);
  };

  const selectProfile = (profile: ProviderProfile) => {
    invoke<string>("activate_provider_profile", { request: { id: profile.id } })
      .then((message) => {
        setEditingId(profile.id);
        setProfileName(profile.name);
        setBaseUrl(profile.baseUrl);
        setBearerToken(profile.bearerToken);
        setIsCreatingProfile(false);
        onMessage(message);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)));
  };

  const deleteProfile = () => {
    if (!editingId) {
      onMessage("请选择要删除的配置档");
      return;
    }
    const name = profileName.trim() || activeProfile?.name || "当前配置档";
    if (!window.confirm(`确定删除“${name}”？删除后会自动切换到其他配置档。`)) {
      return;
    }
    invoke<string>("delete_provider_profile", { request: { id: editingId } })
      .then((message) => {
        onMessage(message);
        setEditingId("");
        setIsCreatingProfile(false);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)));
  };

  const clear = () => {
    if (pendingAction) return;
    setPendingAction("clear");
    onProgress("正在保存官方通道");
    onMessage("正在保存官方通道");
    invoke<string>("clear_provider")
      .then((message) => {
        onMessage(message);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPendingAction("");
        onProgress("");
      });
  };

  const saveOfficialChannel = () => {
    clear();
  };

  return (
    <div className="providerLayout">
      <section className="panel widePanel statusPanel">
        <div className="panelHeader">
          <div className="panelTitle compactTitle">
            <CheckCircle2 size={16} />
            <h2>当前状态</h2>
          </div>
          <code>{provider?.source ?? "~/.codex/config.toml"}</code>
        </div>
        <div className="providerStatusGrid">
          <div className="statusMetric">
            <span>官方登录</span>
            <strong>{provider?.authenticated ? "已检测" : "未检测"}</strong>
          </div>
          <Metric label="当前通道" value={runModeLabel(currentMode)} />
          <Metric label="配置档" value={provider?.profile ?? "默认"} />
          <Metric label="已配置" value={provider?.configured ? "是" : "否"} />
        </div>
        <div className="accountLine">
          <span className={`statusDot ${provider?.authenticated ? "ok" : "warning"}`} />
          <span>登录账号</span>
          <strong>{provider?.accountLabel ?? "未读取到账号信息"}</strong>
        </div>
      </section>

      <section className="panel widePanel modePanel">
        <div className="panelHeader">
          <div className="panelTitle compactTitle">
            <LogIn size={16} />
            <h2>选择通道</h2>
          </div>
          <code>{provider?.authPath ?? "~/.codex/auth.json"}</code>
        </div>
        <div className="modeGrid">
          <ModeCard
            active={selectedChannel === "official"}
            description="使用 Codex/ChatGPT 官方登录，不写入自定义模型供应商。"
            disabled={Boolean(pendingAction)}
            icon={BadgeCheck}
            onClick={() => setSelectedChannel("official")}
            title="官方通道"
          />
          <ModeCard
            active={selectedChannel === "hybridApi"}
            description="保留 Codex/ChatGPT 登录，把模型请求转到当前 API 配置。"
            disabled={Boolean(pendingAction)}
            icon={Network}
            onClick={() => setSelectedChannel("hybridApi")}
            title="混合中转"
          />
        </div>
      </section>

      {customChannelSelected ? (
        <section className="panel widePanel profilePanel">
          <div className="panelTitle">
            <Network size={16} />
            <h2>配置档</h2>
          </div>
          <div className="profileList">
            {visibleProfiles.map((profile) => {
              const selected = isCreatingProfile ? !profile.id : profile.id === activeProfileId;
              return (
                <div className={`profileItem ${selected ? "active" : ""}`} key={profile.id || "new"}>
                  <div className="profileItemHeader">
                    <button className="profileSelectArea" onClick={() => profile.id && selectProfile(profile)} type="button">
                      <strong>{profile.name || "新中转"}</strong>
                      <span>{selected ? "当前配置 · 混合中转" : `混合中转 · ${profile.baseUrl || "未填写 Base URL"}`}</span>
                    </button>
                    {selected && editingId && (
                      <button className="profileDelete" onClick={deleteProfile} type="button">
                        删除
                      </button>
                    )}
                  </div>
                  {selected && (
                    <>
                      <div className="profileFormGrid">
                        <label>
                          <span>配置名称</span>
                          <input value={profileName} onChange={(event) => setProfileName(event.target.value)} placeholder="默认中转" />
                        </label>
                        <label>
                          <span>Base URL</span>
                          <input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://example.com/v1" />
                        </label>
                        <label>
                          <span>API Key</span>
                          <div className="inputWithButton">
                            <input
                              value={bearerToken}
                              onChange={(event) => setBearerToken(event.target.value)}
                              placeholder="sk-..."
                              type={showToken ? "text" : "password"}
                            />
                            <button className="secondary iconButton" onClick={() => setShowToken((value) => !value)} title={showToken ? "隐藏" : "显示"} type="button">
                              {showToken ? <EyeOff size={16} /> : <Eye size={16} />}
                            </button>
                          </div>
                        </label>
                      </div>
                      <div className="profileSaveRow">
                        <button className="primary" disabled={Boolean(pendingAction)} onClick={saveMixedRelay} type="button">
                          {pendingAction === "save" ? "保存中" : "保存"}
                        </button>
                      </div>
                    </>
                  )}
                </div>
              );
            })}
            <button className="addProfile" onClick={newProfile} title="新增配置" type="button">
              <Plus size={18} />
            </button>
          </div>
        </section>
      ) : (
        <section className="panel widePanel officialPanel">
          <div className="panelTitle">
            <BadgeCheck size={16} />
            <h2>官方通道</h2>
          </div>
          <div className="officialBox">
            <strong>使用 Codex/ChatGPT 官方登录</strong>
            <span>不会写入 CodexPilot 模型供应商，也不会使用自定义 API 配置。</span>
          </div>
          <div className="profileSaveRow">
            <button className="primary" disabled={Boolean(pendingAction)} onClick={saveOfficialChannel} type="button">
              {pendingAction === "clear" ? "保存中" : "保存"}
            </button>
          </div>
        </section>
      )}
            </div>
  );
}

function ModeCard({
  active,
  description,
  disabled,
  icon: Icon,
  onClick,
  title,
}: {
  active: boolean;
  description: string;
  disabled: boolean;
  icon: React.ElementType;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      className={`modeCard ${active ? "active" : ""}`}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      <span className="modeIcon">
        <Icon size={18} />
      </span>
      <strong>{title}</strong>
      <span>{description}</span>
    </button>
  );
}

function runModeLabel(mode: RunMode): string {
  if (mode === "hybridApi" || mode === "api") return "混合中转";
  return "官方通道";
}

function RecycleBinView({
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
  const selectedEntries = entries.filter((entry) => selected.includes(entry.token));
  const recoverableSelected = selectedEntries.filter((entry) => entry.recoverable);
  const allSelected = entries.length > 0 && selected.length === entries.length;

  React.useEffect(() => {
    setSelected((current) => current.filter((token) => entries.some((entry) => entry.token === token)));
  }, [entries]);

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
    invoke<string>("restore_recycle_bin_entries", { request: { tokens } })
      .then((message) => {
        onMessage(message);
        setSelected([]);
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
    const confirmed = window.confirm(`确认永久删除选中的 ${selected.length} 条记录？删除后不能恢复。`);
    if (!confirmed) return;
    setPendingAction("delete");
    onProgress("正在永久删除回收站记录");
    onMessage(`正在永久删除 ${selected.length} 条记录`);
    invoke<string>("delete_recycle_bin_entries", { request: { tokens: selected } })
      .then((message) => {
        onMessage(message);
        setSelected([]);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPendingAction("");
        onProgress("");
      });
  };

  return (
    <section className="panel">
      <div className="panelHeader">
        <div className="panelTitle">
          <Trash2 size={16} />
          <h2>已删除会话</h2>
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
            {pendingAction === "delete" ? "删除中" : "永久删除"}
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
                <th>会话 ID</th>
                <th>来源</th>
                <th>删除时间</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.token}>
                  <td>
                    <input
                      checked={selected.includes(entry.token)}
                      onChange={() => toggleOne(entry.token)}
                      type="checkbox"
                      aria-label={`选择 ${entry.title || entry.sessionId}`}
                    />
                  </td>
                  <td>
                    <strong>{entry.title || "未命名会话"}</strong>
                    <span>{entry.backupPath}</span>
                  </td>
                  <td><code>{shortId(entry.sessionId)}</code></td>
                  <td>{entry.schema || "未知"}</td>
                  <td>{formatDeletedAt(entry.deletedAt)}</td>
                  <td>
                    <span className={`pill ${entry.recoverable ? "ok" : "warning"}`}>
                      {entry.status || (entry.recoverable ? "可恢复" : "不可恢复")}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="bodyText">暂无已删除会话。</p>
      )}
    </section>
  );
}

function DiagnosticsView({
  diagnostics,
  onRefresh,
  onMessage,
  onProgress,
}: {
  diagnostics: DiagnosticsSnapshot | null;
  onRefresh: () => void;
  onMessage: (message: string) => void;
  onProgress: (message: string) => void;
}) {
  const [logMessage, setLogMessage] = React.useState("");
  const logs = diagnostics?.logs ?? [];
  const logText = logs.length ? logs.join("\n") : "";

  const collectDiagnostics = () => {
    setLogMessage("正在生成诊断快照");
    onProgress("正在生成诊断快照");
    invoke<string>("collect_diagnostics")
      .then((message) => {
        setLogMessage(message);
        onMessage(message);
        onRefresh();
      })
      .catch((error) => {
        const message = `生成诊断快照失败：${String(error)}`;
        setLogMessage(message);
        onMessage(message);
      })
      .finally(() => onProgress(""));
  };

  const copyLogs = () => {
    if (!logText) {
      setLogMessage("暂无日志可复制");
      return;
    }
    navigator.clipboard.writeText(logText)
      .then(() => setLogMessage("日志已复制"))
      .catch((error) => setLogMessage(`复制失败：${String(error)}`));
  };

  const exportLogs = () => {
    if (!logText) {
      setLogMessage("暂无日志可导出");
      return;
    }
    const blob = new Blob([`${logText}\n`], { type: "application/jsonl;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `codex-pilot-diagnostic-${Date.now()}.jsonl`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
    setLogMessage("日志已导出");
  };

  return (
    <section className="panel">
      <div className="panelHeader">
        <div className="panelTitle">
          <Stethoscope size={16} />
          <h2>检查项</h2>
        </div>
        <div className="buttonRow">
          <button className="primary" onClick={collectDiagnostics} type="button">
            <Stethoscope size={16} />
            生成诊断快照
          </button>
          <button className="secondary" onClick={copyLogs} type="button">
            <Clipboard size={16} />
            复制日志
          </button>
          <button className="secondary" onClick={exportLogs} type="button">
            <Download size={16} />
            导出日志
          </button>
        </div>
      </div>
      <div className="checkList">
        {(diagnostics?.checks ?? []).map((check) => (
          <div className="checkRow" key={check.name}>
            <span className={`pill ${check.status}`}>{check.status}</span>
            <div>
              <strong>{check.name}</strong>
              <p>{check.detail}</p>
            </div>
          </div>
        ))}
        {!diagnostics?.checks.length && <p className="bodyText">暂无诊断数据。</p>}
      </div>
      {logMessage && <p className="formMessage logMessage">{logMessage}</p>}
      <pre className="logBlock">
        {logText || "暂无日志"}
      </pre>
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function shortId(value: string) {
  if (!value) return "-";
  if (value.length <= 18) return value;
  return `${value.slice(0, 8)}…${value.slice(-8)}`;
}

function formatDeletedAt(value: number | null) {
  if (!value) return "-";
  return new Date(value * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function ProgressDialog({ message }: { message: string }) {
  return (
    <div className="progressOverlay" role="status" aria-live="polite">
      <div className="progressDialog">
        <strong>{message}</strong>
        <div className="progressTrack">
          <span />
        </div>
        <p>正在处理，请稍候。</p>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
