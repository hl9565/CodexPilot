import React from "react";
import ReactDOM from "react-dom/client";
import {
  Activity,
  Bot,
  History,
  LogIn,
  Moon,
  Play,
  RefreshCw,
  Stethoscope,
  Sun,
  Terminal,
  RotateCcw,
} from "lucide-react";
import { ProgressDialog, canRunLaunchAction, loadInitialTheme } from "./appSupport";
import { callBackend, isUiPreviewMode } from "./backend";
import { resolveAutoLaunchAction } from "./autoLaunch";
import { DiagnosticsView } from "./views/DiagnosticsView";
import { LaunchView } from "./views/LaunchView";
import { OverviewView } from "./views/OverviewView";
import { ProviderView } from "./views/ProviderView";
import { RecycleBinView } from "./views/RecycleBinView";
import {
  type BackendStatus,
  type CcsProviderSnapshot,
  type DiagnosticsSnapshot,
  type LaunchSnapshot,
  type ProviderSnapshot,
  type RecycleBinSnapshot,
  type Theme,
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
