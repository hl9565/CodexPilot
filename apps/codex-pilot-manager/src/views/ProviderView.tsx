import * as React from "react";
import {
  CheckCircle2,
  CircleHelp,
  Download,
  Eye,
  EyeOff,
  Network,
  Plus,
  RefreshCw,
} from "lucide-react";
import { callBackend } from "../backend";
import { Metric } from "../components/primitives";
import type {
  AuthenticatedBehavior,
  CcsImportResult,
  CcsProviderSnapshot,
  OfficialSnapshotImportResult,
  OfficialSnapshotPrepareResult,
  ProviderProfile,
  ProviderProfileSaveResponse,
  ProviderSnapshot,
  UpstreamProtocol,
} from "../types";

export function ProviderView({
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
