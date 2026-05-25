import React from "react";
import ReactDOM from "react-dom/client";
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
  Moon,
  Play,
  RefreshCw,
  Settings,
  Stethoscope,
  Sun,
  Terminal,
  Eye,
  EyeOff,
  Network,
  RotateCcw,
  Plus,
  CircleHelp,
} from "lucide-react";
import { callBackend, isUiPreviewMode } from "./backend";
import { resolveAutoLaunchAction } from "./autoLaunch";
import { Distribution, Metric, Row } from "./components/primitives";
import { OverviewView } from "./views/OverviewView";
import {
  type AuthenticatedBehavior,
  type BackendStatus,
  type CcsImportResult,
  type CcsProviderSnapshot,
  type DiagnosticsSnapshot,
  type EnhancementSettings,
  type LaunchSnapshot,
  type OfficialSnapshotImportResult,
  type OfficialSnapshotPrepareResult,
  type ProviderCount,
  type ProviderProfile,
  type ProviderProfileSaveResponse,
  type ProviderSnapshot,
  type ProviderSyncSnapshot,
  type RecycleBinBatchResponse,
  type RecycleBinEntry,
  type RecycleBinSnapshot,
  type RunMode,
  type SessionZipExportResult,
  type SessionZipImportMode,
  type SessionZipImportResult,
  type SessionZipInspectResult,
  THEME_STORAGE_KEY,
  type Theme,
  type UpstreamProtocol,
  type ViewId,
} from "./types";
import "./styles.css";

const views: Array<{ id: ViewId; label: string; icon: React.ElementType }> = [
  { id: "overview", label: "总览", icon: Activity },
  { id: "launch", label: "启动与注入", icon: Terminal },
  { id: "provider", label: "模型通道", icon: LogIn },
  { id: "sessions", label: "对话维护", icon: History },
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
  const [theme, setTheme] = React.useState<Theme>(() => loadInitialTheme());
  const [status, setStatus] = React.useState<BackendStatus | null>(null);
  const [appVersion, setAppVersion] = React.useState<string | null>(null);
  const [launch, setLaunch] = React.useState<LaunchSnapshot | null>(null);
  const [provider, setProvider] = React.useState<ProviderSnapshot | null>(null);
  const [ccsProvider, setCcsProvider] = React.useState<CcsProviderSnapshot | null>(null);
  const [recycleBin, setRecycleBin] = React.useState<RecycleBinSnapshot | null>(null);
  const [diagnostics, setDiagnostics] = React.useState<DiagnosticsSnapshot | null>(null);
  const [message, setMessage] = React.useState("就绪");
  const [toast, setToast] = React.useState("");
  const [progressMessage, setProgressMessage] = React.useState("");
  const [launching, setLaunching] = React.useState(false);
  const autoLaunchAttemptedRef = React.useRef(false);
  const autoLaunchFailedRef = React.useRef(false);
  const launchRequestIdRef = React.useRef(0);

  const notify = React.useCallback((value: string) => {
    setMessage(value);
    setToast(value);
  }, []);

  React.useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(""), 3200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.classList.toggle("light", theme === "light");
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  }, [theme]);

  const refresh = React.useCallback((silent = false) => {
    if (!silent) notify("正在刷新");
    Promise.all([
      callBackend<BackendStatus | null>("backend_status")
        .then(setStatus)
        .catch(() => setStatus(null)),
      callBackend<LaunchSnapshot>("launch_snapshot")
        .then(setLaunch)
        .catch(() => setLaunch(null)),
      callBackend<ProviderSnapshot>("provider_snapshot")
        .then(setProvider)
        .catch(() => setProvider(null)),
      callBackend<CcsProviderSnapshot>("ccs_provider_snapshot")
        .then(setCcsProvider)
        .catch(() => setCcsProvider(null)),
      callBackend<RecycleBinSnapshot>("recycle_bin_snapshot")
        .then(setRecycleBin)
        .catch(() => setRecycleBin(null)),
      callBackend<DiagnosticsSnapshot>("diagnostics_snapshot")
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
    let debounceTimer: number | null = null;
    const refreshWhenVisible = () => {
      if (document.visibilityState !== "visible") return;
      if (debounceTimer !== null) {
        window.clearTimeout(debounceTimer);
      }
      debounceTimer = window.setTimeout(() => {
        refresh(true);
        debounceTimer = null;
      }, 500);
    };
    window.addEventListener("focus", refreshWhenVisible);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    return () => {
      if (debounceTimer !== null) {
        window.clearTimeout(debounceTimer);
      }
      window.removeEventListener("focus", refreshWhenVisible);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [refresh]);

  React.useEffect(() => {
    callBackend<string>("app_version")
      .then(setAppVersion)
      .catch(() => setAppVersion(null));
  }, []);

  React.useEffect(() => {
    if (!launch) return;
    const action = resolveAutoLaunchAction({
      actionKind: launch.actionKind,
      autoLaunchOnOpen: launch.autoLaunchOnOpen,
      alreadyAttempted: autoLaunchAttemptedRef.current,
      alreadyFailed: autoLaunchFailedRef.current,
      launching,
      codexInstalled: launch.codexInstalled,
    });
    if (action.kind === "skip") return;
    if (action.markAttempted) {
      autoLaunchAttemptedRef.current = true;
    }
    if (action.kind === "stop") {
      if (action.message) notify(action.message);
      return;
    }
    const requestId = ++launchRequestIdRef.current;
    setLaunching(true);
    setProgressMessage(action.progress);
    notify(action.message);
    callBackend<string>(action.command)
      .then((value) => {
        if (requestId !== launchRequestIdRef.current) return;
        notify(value);
        refresh(true);
      })
      .catch((error) => {
        if (requestId !== launchRequestIdRef.current) return;
        autoLaunchFailedRef.current = true;
        notify(String(error));
      })
      .finally(() => {
        if (requestId !== launchRequestIdRef.current) return;
        setLaunching(false);
        setProgressMessage("");
      });
  }, [launch, launching, notify, refresh]);

  React.useEffect(() => {
    if (!launching || !launch || !progressMessage) return;
    const startingCodex =
      progressMessage.includes("启动 Codex") || progressMessage.includes("启动中");
    if (!startingCodex) return;
    if (launch.actionKind !== "reinject" && launch.actionKind !== "running") return;

    launchRequestIdRef.current += 1;
    setLaunching(false);
    setProgressMessage("");
    notify(
      launch.actionKind === "running"
        ? "CodexPilot 已连接。"
        : "Codex 已启动，但 CodexPilot 还没接上；现在可以直接重新注入。"
    );
  }, [launch, launching, progressMessage, notify]);

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
    const requestId = ++launchRequestIdRef.current;
    setLaunching(true);
    setProgressMessage(progress);
    notify(progress);
    callBackend<string>(command)
      .then((value) => {
        if (requestId !== launchRequestIdRef.current) return;
        notify(value);
        refresh();
      })
      .catch((error) => {
        if (requestId !== launchRequestIdRef.current) return;
        notify(String(error));
      })
      .finally(() => {
        if (requestId !== launchRequestIdRef.current) return;
        setLaunching(false);
        setProgressMessage("");
      });
  };

  return (
    <main className={`shell ${theme}`}>
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
            <h1>{views.find((view) => view.id === activeView)?.label}</h1>
          </div>
          <div className="headerActions">
            <span className="message">{message}</span>
            <button
              className="secondary iconButton"
              onClick={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
              title={theme === "dark" ? "切换到浅色模式" : "切换到夜晚模式"}
              type="button"
            >
              {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
            </button>
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
            ccsProvider={ccsProvider}
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
  const [autoLaunchOnOpen, setAutoLaunchOnOpen] = React.useState(false);
  const [autoSyncSessionsOnLaunch, setAutoSyncSessionsOnLaunch] = React.useState(false);
  const [enhancementSettings, setEnhancementSettings] = React.useState<EnhancementSettings>({
    enabled: true,
    timeline: true,
    inlineActions: true,
    scrollRestore: true,
  });
  const [saveMessage, setSaveMessage] = React.useState("");
  const [enhancementMessage, setEnhancementMessage] = React.useState("");
  const [enhancementSaving, setEnhancementSaving] = React.useState(false);
  const backendState = backendStatusLabel(status);
  const connectionState = launch?.debugReachable ? "可直接注入" : launch?.codexRunning ? "需要重启注入" : "可启动";

  React.useEffect(() => {
    if (!launch) return;
    setAppPath(launch.requestedAppPath || launch.appPath || "");
    setDebugPort(String(launch.debugPort));
    setHelperPort(String(launch.helperPort));
    setAutoLaunchOnOpen(Boolean(launch.autoLaunchOnOpen));
    setAutoSyncSessionsOnLaunch(Boolean(launch.autoSyncSessionsOnLaunch));
  }, [launch]);

  React.useEffect(() => {
    callBackend<EnhancementSettings>("enhancement_settings_snapshot")
      .then(setEnhancementSettings)
      .catch((error) => setEnhancementMessage(`读取页面增强设置失败：${String(error)}`));
  }, []);

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
    callBackend<string>("save_launch_preferences", {
      request: {
        appPath,
        debugPort: debug,
        helperPort: helper,
        autoLaunchOnOpen,
        autoSyncSessionsOnLaunch,
      },
    })
      .then((message) => {
        setSaveMessage(message);
        onRefresh();
      })
      .catch((error) => setSaveMessage(String(error)));
  };

  const updateEnhancementSettings = (patch: Partial<EnhancementSettings>) => {
    const next = { ...enhancementSettings, ...patch };
    setEnhancementSettings(next);
    setEnhancementSaving(true);
    setEnhancementMessage("正在保存页面增强设置");
    callBackend<string>("save_enhancement_settings", { request: next })
      .then(setEnhancementMessage)
      .catch((error) => {
        setEnhancementMessage(`保存页面增强设置失败：${String(error)}`);
        return callBackend<EnhancementSettings>("enhancement_settings_snapshot")
          .then(setEnhancementSettings)
          .catch(() => {});
      })
      .finally(() => setEnhancementSaving(false));
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
        <div className="panelHeader">
          <div className="panelTitle">
            <Settings size={16} />
            <h2>启动偏好</h2>
          </div>
        </div>
        <div className="launchPreferences">
          <label className="preferenceField pathField">
            <span>Codex 应用路径</span>
            <input
              value={appPath}
              onChange={(event) => setAppPath(event.target.value)}
              placeholder="/Applications/Codex.app"
            />
          </label>
          <div className="preferenceGrid">
            <label className="preferenceField">
              <span>调试端口</span>
              <input
                inputMode="numeric"
                value={debugPort}
                onChange={(event) => setDebugPort(event.target.value)}
                placeholder="9688"
              />
            </label>
            <label className="preferenceField">
              <span>后端端口</span>
              <input
                inputMode="numeric"
                value={helperPort}
                onChange={(event) => setHelperPort(event.target.value)}
                placeholder="58888"
              />
            </label>
          </div>
          <div className="preferenceFooter">
            <div className="preferenceChecks">
              <label className="checkboxRow compactCheckbox">
                <input
                  checked={autoLaunchOnOpen}
                  onChange={(event) => setAutoLaunchOnOpen(event.target.checked)}
                  type="checkbox"
                />
                <span>打开 CodexPilot 时自动启动或注入 Codex</span>
              </label>
              <label className="checkboxRow compactCheckbox">
                <input
                  checked={autoSyncSessionsOnLaunch}
                  onChange={(event) => setAutoSyncSessionsOnLaunch(event.target.checked)}
                  type="checkbox"
                />
                <span>启动后自动同步会话</span>
              </label>
              <p className="formHint">
                启动或注入成功后，按当前生效通道目标自动检查并同步历史会话归属。
              </p>
            </div>
            <div className="buttonRow compactButtonRow">
              <button className="primary" onClick={savePreferences} type="button">保存偏好</button>
              <button className="secondary" onClick={() => setAppPath("")} type="button">使用自动探测</button>
            </div>
          </div>
          {saveMessage && <p className="formMessage">{saveMessage}</p>}
        </div>
      </section>

      <section className="panel enhancementPanel">
        <div className="panelHeader">
          <div className="panelTitle">
            <Settings size={16} />
            <h2>页面增强</h2>
          </div>
        </div>
        <p className="formHint enhancementIntro">
          控制注入到 Codex 页面里的可见增强。关闭后不会影响模型通道、对话维护和诊断。
        </p>
        <div className="enhancementList">
          <SwitchRow
            checked={enhancementSettings.enabled}
            description="关闭后隐藏 Pilot 页面入口、Timeline、行内操作和阅读位置恢复。"
            disabled={enhancementSaving}
            label="页面增强总开关"
            onChange={(checked) => updateEnhancementSettings({ enabled: checked })}
          />
          <div className={`enhancementChildren ${!enhancementSettings.enabled ? "disabled" : ""}`}>
            <SwitchRow
              checked={enhancementSettings.timeline}
              description="在长对话右侧显示问题跳转点。"
              disabled={!enhancementSettings.enabled || enhancementSaving}
              label="Timeline"
              onChange={(checked) => updateEnhancementSettings({ timeline: checked })}
            />
            <SwitchRow
              checked={enhancementSettings.inlineActions}
              description="在会话列表和归档列表显示 Markdown、HTML 导出与删除操作。"
              disabled={!enhancementSettings.enabled || enhancementSaving}
              label="行内导出和删除"
              onChange={(checked) => updateEnhancementSettings({ inlineActions: checked })}
            />
            <SwitchRow
              checked={enhancementSettings.scrollRestore}
              description="切换会话后回到上次阅读位置。"
              disabled={!enhancementSettings.enabled || enhancementSaving}
              label="滚动恢复"
              onChange={(checked) => updateEnhancementSettings({ scrollRestore: checked })}
            />
          </div>
        </div>
        {!enhancementSettings.enabled && (
          <p className="formMessage subtleMessage">页面增强已关闭，下面的分项设置会在重新打开后继续生效。</p>
        )}
        {enhancementMessage && <p className="formMessage">{enhancementMessage}</p>}
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div className="panelTitle">
            <CheckCircle2 size={16} />
            <h2>运行环境</h2>
          </div>
        </div>
        <div className="rows">
          <Row label="Codex 应用" value={launch?.appPath ?? "未发现"} />
          <Row label="偏好路径" value={launch?.requestedAppPath || "自动探测"} />
          <Row label="调试端口" value={String(launch?.debugPort ?? "-")} />
          <Row label="连接端口" value={String(launch?.helperPort ?? "-")} />
          <Row label="调试端口状态" value={launch?.debugReachable ? "可连接" : "未连接"} />
          <Row label="连接服务状态" value={launch?.helperReachable ? "可连接" : "未连接"} />
        </div>
        <pre className="commandBlock">
          {launch?.commandPreview.length ? launch.commandPreview.join(" ") : "暂无启动命令"}
        </pre>
      </section>
    </div>
  );
}

function SwitchRow({
  checked,
  description,
  disabled,
  label,
  onChange,
}: {
  checked: boolean;
  description: string;
  disabled?: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className={`switchRow ${disabled ? "disabled" : ""}`}>
      <span className="switchText">
        <strong>{label}</strong>
        <span>{description}</span>
      </span>
      <input
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        type="checkbox"
      />
    </label>
  );
}

function ProviderView({
  ccsProvider,
  provider,
  onMessage,
  onProgress,
  onRefresh,
}: {
  ccsProvider: CcsProviderSnapshot | null;
  provider: ProviderSnapshot | null;
  onMessage: (message: string) => void;
  onProgress: (message: string) => void;
  onRefresh: () => void;
}) {
  const profiles = provider?.profiles ?? [];
  const activeProfileId = provider?.activeProfileId || profiles[0]?.id || "";
  const activeProfile = profiles.find((profile) => profile.id === activeProfileId) ?? profiles[0] ?? null;
  const [editingId, setEditingId] = React.useState("");
  const [profileName, setProfileName] = React.useState("");
  const [baseUrl, setBaseUrl] = React.useState("");
  const [bearerToken, setBearerToken] = React.useState("");
  const [upstreamProtocol, setUpstreamProtocol] = React.useState<UpstreamProtocol>("responses");
  const [authenticatedBehavior, setAuthenticatedBehavior] = React.useState<AuthenticatedBehavior>("relay");
  const [showToken, setShowToken] = React.useState(false);
  const [isCreatingProfile, setIsCreatingProfile] = React.useState(false);
  const [pendingAction, setPendingAction] = React.useState("");
  const [refreshingCcs, setRefreshingCcs] = React.useState(false);
  const [importingCcs, setImportingCcs] = React.useState(false);
  const [importingOfficialSnapshot, setImportingOfficialSnapshot] = React.useState(false);
  const [preparingOfficialSnapshot, setPreparingOfficialSnapshot] = React.useState(false);
  const [showOfficialSnapshotHelp, setShowOfficialSnapshotHelp] = React.useState(false);
  const [pendingDeleteId, setPendingDeleteId] = React.useState("");
  const editingProfile = profiles.find((profile) => profile.id === editingId) ?? null;
  const visibleProfiles: ProviderProfile[] = isCreatingProfile
    ? [{
        id: "",
        name: profileName || "新通道",
        baseUrl,
        bearerToken,
        mode: activeProfile?.mode ?? "hybridApi",
        upstreamProtocol,
        authenticatedBehavior,
      }]
    : profiles;

  React.useEffect(() => {
    if (isCreatingProfile || !editingProfile) return;
    setProfileName(editingProfile.name);
    setBaseUrl(editingProfile.baseUrl);
    setBearerToken(editingProfile.bearerToken);
    setUpstreamProtocol(editingProfile.upstreamProtocol ?? "responses");
    setAuthenticatedBehavior(editingProfile.authenticatedBehavior ?? "relay");
  }, [editingProfile?.id, isCreatingProfile]);

  const saveProfile = () => {
    if (pendingAction) return;
    if (!profileName.trim() || !baseUrl.trim() || !bearerToken.trim()) {
      onMessage("配置名称、Base URL 和 API Key 不能为空");
      return;
    }
    setPendingAction("save");
    onProgress("正在保存配置");
    onMessage("正在保存配置");
    callBackend<ProviderProfileSaveResponse>("save_provider_profile", {
      request: {
        id: editingId || null,
        name: profileName,
        baseUrl,
        bearerToken,
        mode: editingProfile?.mode ?? activeProfile?.mode ?? "hybridApi",
        upstreamProtocol,
        authenticatedBehavior,
        activate: !editingId && !activeProfile,
      },
    })
      .then((saveResult) => {
        setEditingId(saveResult.id);
        setIsCreatingProfile(false);
        onMessage(saveResult.message);
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
    setProfileName("新通道");
    setBaseUrl("");
    setBearerToken("");
    setUpstreamProtocol("responses");
    setAuthenticatedBehavior("relay");
    setShowToken(false);
    setPendingDeleteId("");
    setIsCreatingProfile(true);
  };

  const selectProfile = (profile: ProviderProfile) => {
    onProgress("正在应用配置档");
    callBackend<string>("activate_provider_profile", { request: { id: profile.id } })
      .then((message) => {
        setIsCreatingProfile(false);
        setPendingDeleteId("");
        onMessage(message);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => onProgress(""));
  };

  const startEditingProfile = (profile: ProviderProfile) => {
    setEditingId(profile.id);
    setProfileName(profile.name);
    setBaseUrl(profile.baseUrl);
    setBearerToken(profile.bearerToken);
    setUpstreamProtocol(profile.upstreamProtocol ?? "responses");
    setAuthenticatedBehavior(profile.authenticatedBehavior ?? "relay");
    setShowToken(false);
    setPendingDeleteId("");
    setIsCreatingProfile(false);
  };

  const refreshCcsProviders = () => {
    if (refreshingCcs || importingCcs) return;
    setRefreshingCcs(true);
    onProgress("正在刷新 CCSwitch 配置");
    callBackend<CcsProviderSnapshot>("ccs_provider_snapshot")
      .then((snapshot) => {
        onMessage(snapshot.message);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setRefreshingCcs(false);
        onProgress("");
      });
  };

  const importCcsProviders = () => {
    if (refreshingCcs || importingCcs) return;
    setImportingCcs(true);
    onProgress("正在导入 CCSwitch 配置");
    onMessage("正在导入 CCSwitch 配置");
    callBackend<CcsImportResult>("import_ccs_provider_profiles")
      .then((result) => {
        onMessage(result.message);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setImportingCcs(false);
        onProgress("");
      });
  };

  const deleteProfile = () => {
    if (!editingId) {
      onMessage("请选择要删除的配置档");
      return;
    }
    if (pendingDeleteId !== editingId) {
      setPendingDeleteId(editingId);
      return;
    }
    callBackend<string>("delete_provider_profile", { request: { id: editingId } })
      .then((message) => {
        onMessage(message);
        setEditingId("");
        setIsCreatingProfile(false);
        setPendingDeleteId("");
        onRefresh();
      })
      .catch((error) => onMessage(String(error)));
  };

  const cancelDelete = () => {
    setPendingDeleteId("");
  };

  const importOfficialSnapshot = () => {
    if (importingOfficialSnapshot) return;
    setImportingOfficialSnapshot(true);
    onProgress("正在导入官方原版快照");
    callBackend<OfficialSnapshotImportResult>("import_official_snapshot_from_backup")
      .then((result) => {
        onMessage(result.message);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setImportingOfficialSnapshot(false);
        onProgress("");
      });
  };

  const prepareOfficialSnapshotAfterClearingRelay = () => {
    if (preparingOfficialSnapshot) return;
    setPreparingOfficialSnapshot(true);
    onProgress("正在停止中转并准备官方原版恢复点");
    callBackend<OfficialSnapshotPrepareResult>("prepare_official_snapshot_after_clearing_relay")
      .then((result) => {
        onMessage(result.message);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPreparingOfficialSnapshot(false);
        onProgress("");
      });
  };

  return (
    <div className="providerLayout">
      <section className="panel widePanel statusPanel">
        <div className="panelHeader">
          <div className="panelTitle compactTitle">
            <CheckCircle2 size={16} />
            <h2>通道状态</h2>
          </div>
          <code>{provider?.source ?? "~/.codex/config.toml"}</code>
        </div>
        <div className="providerStatusGrid">
          <div className="statusMetric">
            <span>官方登录</span>
            <strong>{provider?.authenticated ? "已检测" : "未检测"}</strong>
          </div>
          <Metric label="当前方式" value={provider?.routeLabel ?? "自动中转"} />
          <Metric label="配置档" value={provider?.profile ?? "默认"} />
          <Metric label="已配置" value={provider?.configured ? "是" : "否"} />
          <div className="recoveryMetric">
            <div className="recoveryMetricHeader">
              <span>官方恢复点</span>
              <button
                aria-expanded={showOfficialSnapshotHelp}
                className="helpIconButton"
                onClick={() => setShowOfficialSnapshotHelp((value) => !value)}
                type="button"
              >
                <CircleHelp size={14} />
              </button>
            </div>
            <strong>{provider?.officialSnapshotAvailable ? "已准备" : "未准备"}</strong>
          </div>
        </div>
        <div className="accountLine">
          <span className={`statusDot ${provider?.authenticated ? "ok" : "warning"}`} />
          <span>登录账号</span>
          <strong>{provider?.accountLabel ?? "未读取到账号信息"}</strong>
        </div>
        {!provider?.officialSnapshotAvailable && showOfficialSnapshotHelp ? (
          <div className="helpPopover">
            <p>官方原版恢复点用于在登录态下切回官方原版。</p>
            <p>当前还没有可用恢复点，不影响现在继续使用自动中转。</p>
            <p>你可以从历史备份导入，或先停止中转后再准备恢复点。</p>
            <div className="noticeActions">
              {provider?.backupSnapshotAvailable ? (
                <button
                  className="secondary"
                  disabled={importingOfficialSnapshot || preparingOfficialSnapshot}
                  onClick={importOfficialSnapshot}
                  type="button"
                >
                  {importingOfficialSnapshot ? "导入中" : "从备份导入"}
                </button>
              ) : null}
              <button
                className="secondary"
                disabled={preparingOfficialSnapshot || importingOfficialSnapshot}
                onClick={prepareOfficialSnapshotAfterClearingRelay}
                type="button"
              >
                {preparingOfficialSnapshot ? "准备中" : "停止中转后准备"}
              </button>
            </div>
          </div>
        ) : null}
      </section>

      <section className="panel widePanel profilePanel">
        <div className="panelHeader">
          <div className="panelTitle">
            <Network size={16} />
            <h2>配置档</h2>
          </div>
          <code>{provider?.authPath ?? "~/.codex/auth.json"}</code>
        </div>
        <div className="profileList">
          <div className="ccsImportRow">
            <div className="ccsImportMeta">
              <strong>CCSwitch 配置</strong>
              <span>{ccsProviderSummary(ccsProvider)}</span>
            </div>
            <div className="ccsImportActions">
              <button
                className="secondary"
                disabled={refreshingCcs || importingCcs}
                onClick={refreshCcsProviders}
                type="button"
              >
                <RefreshCw size={16} />
                {refreshingCcs ? "刷新中" : "刷新"}
              </button>
              <button
                className="secondary"
                disabled={Boolean(refreshingCcs || importingCcs || !ccsProvider?.importableCount)}
                onClick={importCcsProviders}
                type="button"
              >
                <Download size={16} />
                {importingCcs ? "导入中" : "导入"}
              </button>
            </div>
          </div>
          {visibleProfiles.map((profile) => {
              const selected = isCreatingProfile ? !profile.id : profile.id === activeProfileId;
              const editing = !isCreatingProfile && profile.id === editingId;
              return (
                <div className={`profileItem ${selected ? "active" : ""} ${editing ? "editing" : ""}`} key={profile.id || "new"}>
                  {editing ? (
                    <div className="profileEditorHeader">
                      <div className="profileEditorTitle">
                        {selected ? <span className="pill ok">当前配置</span> : null}
                        <span>正在编辑</span>
                      </div>
                      {editingId ? (
                        <div className="profileEditorActions">
                          <button className="secondary" onClick={() => setEditingId("")} type="button">
                            收起编辑
                          </button>
                          <button className={`profileDelete ${pendingDeleteId === editingId ? "dangerActive" : ""}`} onClick={deleteProfile} type="button">
                            {pendingDeleteId === editingId ? "确认删除" : "删除配置"}
                          </button>
                          {pendingDeleteId === editingId ? (
                            <button className="secondary" onClick={cancelDelete} type="button">
                              取消
                            </button>
                          ) : null}
                        </div>
                      ) : null}
                    </div>
                  ) : (
                    <div className="profileItemHeader">
                      <button className="profileSelectArea" onClick={() => profile.id && selectProfile(profile)} type="button">
                        <strong>{profile.name || "新中转"}</strong>
                        <span>{`${upstreamProtocolLabel(profile.upstreamProtocol)} · ${profile.baseUrl || "未填写 Base URL"}`}</span>
                      </button>
                      <div className="profileItemActions">
                        {selected ? <span className="pill ok">当前配置</span> : null}
                        {!selected ? (
                          <button
                            className="secondary"
                            disabled={Boolean(pendingAction)}
                            onClick={() => selectProfile(profile)}
                            type="button"
                          >
                            启用
                          </button>
                        ) : null}
                        <button className="secondary" onClick={() => startEditingProfile(profile)} type="button">
                          编辑
                        </button>
                      </div>
                    </div>
                  )}
                  {(editing || isCreatingProfile) && (
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
                        <label>
                          <span>上游协议</span>
                          <select value={upstreamProtocol} onChange={(event) => setUpstreamProtocol(event.target.value as UpstreamProtocol)}>
                            <option value="responses">Responses API</option>
                            <option value="chatCompletions">Chat Completions</option>
                          </select>
                        </label>
                      </div>
                      <SwitchRow
                        checked={authenticatedBehavior === "officialDirect"}
                        description="勾选后，存在官方登录态时优先恢复官方原版；没有快照或未登录时会自动退化为中转。"
                        label="登录态存在时改走官方原版"
                        onChange={(checked) => setAuthenticatedBehavior(checked ? "officialDirect" : "relay")}
                      />
                      {upstreamProtocol === "chatCompletions" ? (
                        <div className="officialBox">
                          <strong>本地协议转换</strong>
                          <span>Codex 仍连接本地 Responses 入口，CodexPilot 会把请求转换到 Chat Completions 上游。</span>
                        </div>
                      ) : null}
                      <div className="profileSaveRow">
                        <button
                          className="primary"
                          disabled={Boolean(pendingAction)}
                          onClick={saveProfile}
                          type="button"
                        >
                          {pendingAction === "save" ? "保存中" : "保存配置"}
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
            </div>
  );
}

function runModeLabel(mode: RunMode): string {
  if (mode === "hybridApi") return "混合中转";
  if (mode === "api") return "传统中转";
  return "官方通道";
}

function upstreamProtocolLabel(protocol: UpstreamProtocol): string {
  if (protocol === "chatCompletions") return "Chat Completions";
  if (protocol === "anthropicMessages") return "Anthropic Messages";
  return "Responses API";
}

function ccsProviderSummary(snapshot: CcsProviderSnapshot | null): string {
  if (!snapshot) return "尚未读取 CCSwitch 配置。";
  if (snapshot.status === "error") return snapshot.message;
  if (snapshot.status === "missing") return "未找到 CCSwitch 数据库。";
  if (snapshot.status === "empty") return "未发现 CCSwitch Codex 配置。";
  return snapshot.message;
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
    const confirmed = window.confirm(`确认永久删除选中的 ${selected.length} 条记录？删除后不能恢复。`);
    if (!confirmed) return;
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
  const logLines = React.useMemo(() => logs.map(formatDiagnosticLogLine), [logs]);

  const collectDiagnostics = () => {
    setLogMessage("正在生成诊断快照");
    onProgress("正在生成诊断快照");
    callBackend<string>("collect_diagnostics")
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
    <div className="diagnosticsLayout">
    <section className="panel">
      <div className="panelHeader">
        <div className="panelTitle">
          <Stethoscope size={16} />
          <h2>运行检查</h2>
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
        {logLines.length ? logLines.join("\n") : "暂无日志"}
      </pre>
    </section>
    </div>
  );
}

function formatDiagnosticLogLine(line: string): string {
  const text = String(line || "");
  try {
    const parsed = JSON.parse(text) as { ts?: number | string };
    const prefix = formatDiagnosticTimestamp(parsed?.ts);
    return prefix ? `${prefix} ${text}` : text;
  } catch (_error) {
    return text;
  }
}

function formatDiagnosticTimestamp(value: unknown): string {
  const raw = typeof value === "string" || typeof value === "number" ? Number(value) : Number.NaN;
  if (!Number.isFinite(raw) || raw <= 0) return "";
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (part: number, size = 2) => String(part).padStart(size, "0");
  return [
    date.getFullYear(),
    "-",
    pad(date.getMonth() + 1),
    "-",
    pad(date.getDate()),
    " ",
    pad(date.getHours()),
    ":",
    pad(date.getMinutes()),
    ":",
    pad(date.getSeconds()),
    ".",
    pad(date.getMilliseconds(), 3),
  ].join("");
}

function shortId(value: string) {
  if (!value) return "-";
  if (value.length <= 18) return value;
  return `${value.slice(0, 8)}…${value.slice(-8)}`;
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

function loadInitialTheme(): Theme {
  if (typeof window === "undefined") return "light";
  return window.localStorage.getItem(THEME_STORAGE_KEY) === "dark" ? "dark" : "light";
}

function providerSyncSummary(snapshot: ProviderSyncSnapshot) {
  const pending = snapshot.rolloutRewriteNeeded + snapshot.sqliteProviderRowsNeedingSync;
  if (pending > 0) return `预计影响 ${pending} 项`;
  return "历史会话记录一致";
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

if (isUiPreviewMode) {
  document.body.classList.add("uiPreviewMode");
}

const app = (
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

ReactDOM.createRoot(document.getElementById("root")!).render(
  isUiPreviewMode ? (
    <div className="previewStage">
      <div className="previewWindow" aria-label="CodexPilot 1120 by 760 preview">
        {app}
      </div>
    </div>
  ) : (
    app
  ),
);
