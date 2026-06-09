import * as React from "react";
import { CheckCircle2, Gauge, Settings } from "lucide-react";
import { callBackend } from "../backend";
import { Metric, Row } from "../components/primitives";
import type { BackendStatus, EnhancementSettings, LaunchSnapshot } from "../types";

function backendStatusLabel(status: BackendStatus | null): string {
  if (!status) return "未连接";
  if (status.status === "running") return "已连接";
  return status.status || "未连接";
}

export function LaunchView({
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
    pluginEntryUnlock: true,
    forcePluginInstall: true,
    fastGlobalMode: true,
  });
  const [saveMessage, setSaveMessage] = React.useState("");
  const [enhancementMessage, setEnhancementMessage] = React.useState("");
  const [enhancementSaving, setEnhancementSaving] = React.useState(false);
  const backendState = backendStatusLabel(status);
  const connectionState = (() => {
    switch (launch?.actionKind) {
      case "running":
        return "已连接";
      case "launching":
        return "启动中";
      case "reinject":
        return "可直接注入";
      case "restart":
        return "需要重启注入";
      case "launch":
        return "可启动";
      case "unavailable":
        return "未配置";
      default:
        return "未知";
    }
  })();

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
                启动或注入成功后，按当前配置的 Provider 自动检查并同步历史会话归属。
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
          控制注入到 Codex 页面里的可见增强。关闭后不会影响对话维护、对话同步和诊断。
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
            <SwitchRow
              checked={enhancementSettings.pluginEntryUnlock}
              description="未登录 ChatGPT（API Key 模式）时解锁原生插件入口，免登录使用插件。"
              disabled={!enhancementSettings.enabled || enhancementSaving}
              label="插件入口解锁"
              onChange={(checked) => updateEnhancementSettings({ pluginEntryUnlock: checked })}
            />
            <SwitchRow
              checked={enhancementSettings.forcePluginInstall}
              description="解除 App unavailable / 应用不可用导致的插件安装按钮禁用。"
              disabled={!enhancementSettings.enabled || enhancementSaving}
              label="特殊插件强制安装"
              onChange={(checked) => updateEnhancementSettings({ forcePluginInstall: checked })}
            />
            <SwitchRow
              checked={enhancementSettings.fastGlobalMode}
              description="所有对话默认使用 Fast（priority）服务档位；关闭后恢复按对话/草稿手动控制。"
              disabled={!enhancementSettings.enabled || enhancementSaving}
              label="全局 Fast"
              onChange={(checked) => updateEnhancementSettings({ fastGlobalMode: checked })}
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
