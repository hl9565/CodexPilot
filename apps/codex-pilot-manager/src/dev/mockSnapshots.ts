type MockCommandHandler = (args?: unknown) => unknown;

const nowSeconds = Math.floor(Date.now() / 1000);

const commandHandlers: Record<string, MockCommandHandler> = {
  backend_status: () => ({
    status: "running",
    version: "0.9.5-preview",
  }),
  launch_snapshot: () => ({
    appPath: "/Applications/Codex.app",
    requestedAppPath: "",
    debugPort: 9688,
    helperPort: 58888,
    autoLaunchOnOpen: false,
    autoSyncSessionsOnLaunch: false,
    ready: true,
    state: "ready",
    actionKind: "reinject",
    actionLabel: "重新注入",
    helperReachable: true,
    debugReachable: true,
    codexRunning: true,
    detail: "Codex 已运行，调试端口和后端服务均可连接。",
    commandPreview: [
      "/Applications/Codex.app/Contents/MacOS/Codex",
      "--remote-debugging-port=9688",
      "--codex-pilot-helper-port=58888",
    ],
  }),
  provider_snapshot: () => ({
    activeProvider: "acme-relay",
    mode: "hybridApi",
    profile: "团队中转",
    source: "~/.codex/config.toml",
    authPath: "~/.codex/auth.json",
    configured: true,
    authenticated: true,
    accountLabel: "pilot@example.com",
    routeLabel: "自动中转（登录态）",
    statusMessage: "当前按登录态使用自动中转。",
    degraded: false,
    officialSnapshotAvailable: true,
    backupSnapshotAvailable: true,
    profiles: [
      {
        id: "team-relay",
        name: "团队中转",
        baseUrl: "https://relay.example.com/v1",
        bearerToken: "preview-team-relay-key",
        mode: "hybridApi",
        upstreamProtocol: "responses",
        authenticatedBehavior: "relay",
      },
      {
        id: "backup-relay",
        name: "备用中转",
        baseUrl: "https://backup-relay.example.com/v1",
        bearerToken: "preview-backup-relay-key",
        mode: "api",
        upstreamProtocol: "chatCompletions",
        authenticatedBehavior: "officialDirect",
      },
    ],
    activeProfileId: "team-relay",
  }),
  ccs_provider_snapshot: () => ({
    dbPath: "/Users/huanglin/.cc-switch/cc-switch.db",
    availableCount: 3,
    importableCount: 2,
    status: "ready",
    message: "已发现 2 个可导入配置。",
  }),
  recycle_bin_snapshot: () => ({
    entries: [
      {
        token: "preview-token-restore",
        sessionId: "018f4d2a7adf7c37a9f0d9c7fbb10291",
        title: "修复 provider 配置保存",
        projectCwd: "/Users/huanglin/code/github/CodexPilot",
        schema: "codex",
        dbPath: "~/Library/Application Support/Codex/sessions.db",
        backupPath: "~/.codex-pilot/recycle-bin/provider-session.json",
        deletedAt: nowSeconds - 3600,
        lastActiveAt: nowSeconds - 5400,
        recoverable: true,
        status: "可恢复",
      },
      {
        token: "preview-token-missing",
        sessionId: "018f4d2a7adf7c37a9f0d9c7fbb10292",
        title: "旧版诊断实验",
        projectCwd: "/Users/huanglin/code/github/legacy-tooling",
        schema: "legacy",
        dbPath: "~/Library/Application Support/Codex/sessions.db",
        backupPath: "~/.codex-pilot/recycle-bin/legacy-session.json",
        deletedAt: nowSeconds - 86400,
        lastActiveAt: nowSeconds - 172800,
        recoverable: false,
        status: "备份缺失",
      },
    ],
  }),
  diagnostics_snapshot: () => ({
    checks: [
      {
        name: "后端服务",
        status: "ok",
        detail: "本地 helper 端口 58888 可连接。",
      },
      {
        name: "Codex 调试端口",
        status: "ok",
        detail: "Chrome DevTools Protocol 已响应。",
      },
      {
        name: "注入脚本版本",
        status: "warning",
        detail: "预览数据提示：下次真实运行时建议重新注入。",
      },
    ],
    logs: [
      JSON.stringify({ level: "info", message: "preview helper reachable", port: 58888 }),
      JSON.stringify({ level: "warn", message: "preview injection refresh recommended" }),
    ],
  }),
  provider_sync_snapshot: () => ({
    targetProvider: "CodexPilot",
    currentProvider: "acme-relay",
    availableProviders: ["CodexPilot", "acme-relay", "openai"],
    rolloutFiles: 42,
    rolloutRewriteNeeded: 18,
    sqliteRows: 44,
    sqliteProviderRowsNeedingSync: 19,
    sqliteTotalUpdatesNeeded: 21,
    rolloutProviders: [
      { provider: "openai", count: 18 },
      { provider: "CodexPilot", count: 16 },
      { provider: "acme-relay", count: 8 },
    ],
    sqliteProviders: [
      { provider: "openai", count: 19 },
      { provider: "CodexPilot", count: 17 },
      { provider: "acme-relay", count: 8 },
    ],
  }),
  app_version: () => "0.9.5-preview",
  launch_codex: () => "预览模式：已模拟启动 Codex",
  reinject_codex: () => "预览模式：已模拟重新注入 CodexPilot",
  restart_codex_and_inject: () => "预览模式：已模拟重启并注入 Codex",
  save_launch_preferences: () => "预览模式：启动偏好已保存",
  enhancement_settings_snapshot: () => ({
    enabled: true,
    timeline: true,
    inlineActions: true,
    scrollRestore: true,
  }),
  save_enhancement_settings: () => "预览模式：页面增强设置已保存，重新注入后生效。",
  save_provider_profile: () => ({
    id: "team-relay",
    message: "预览模式：配置档已保存",
  }),
  apply_provider: () => "预览模式：模型通道已应用",
  activate_provider_profile: () => "预览模式：配置档已切换",
  delete_provider_profile: () => "预览模式：配置档已删除",
  clear_provider: () => "预览模式：已切换为官方通道",
  import_ccs_provider_profiles: () => ({
    importedCount: 2,
    skippedCount: 1,
    renamedCount: 1,
    provider: commandHandlers.provider_snapshot(),
    ccs: {
      dbPath: "/Users/huanglin/.cc-switch/cc-switch.db",
      availableCount: 3,
      importableCount: 0,
      status: "ready",
      message: "已发现 0 个可导入配置。",
    },
    message: "预览模式：已导入 2 个 CCSwitch 配置，跳过 1 个，重命名 1 个。",
  }),
  import_official_snapshot_from_backup: () => ({
    message: "预览模式：已从备份导入官方原版快照。",
    provider: commandHandlers.provider_snapshot(),
  }),
  sync_provider_sessions: () => "预览模式：Provider Sync 完成，目标 CodexPilot，会话文件 18 个，数据库行 19 条。",
  restore_recycle_bin_entries: () => ({
    message: "预览模式：已恢复所选会话",
    succeededTokens: ["preview-token-restore"],
    failed: [],
  }),
  delete_recycle_bin_entries: () => ({
    message: "预览模式：已永久删除所选记录",
    succeededTokens: ["preview-token-restore", "preview-token-missing"],
    failed: [],
  }),
  export_session_zip: () => ({
    zipPath: "/Users/huanglin/.codex/backups_state/session-zip/codex-sessions-backup-1748073600.zip",
    manifest: {
      version: 1,
      product: "CodexPilot",
      exportedAt: "1748073600000",
      exportedAtMs: 1748073600000,
      includes: {
        sessions: true,
        archivedSessions: true,
        stateSqlite: true,
      },
      counts: {
        sessionFiles: 128,
        archivedSessionFiles: 14,
      },
    },
  }),
  pick_session_zip_save_path: () => "/Users/huanglin/Downloads/codex-sessions-backup-demo.zip",
  pick_session_zip_file: () => "/Users/huanglin/Downloads/codex-sessions-backup-demo.zip",
  inspect_session_zip: () => ({
    zipPath: "/Users/huanglin/Downloads/codex-sessions-backup-demo.zip",
    manifest: {
      version: 1,
      product: "CodexPilot",
      exportedAt: "1748073600000",
      exportedAtMs: 1748073600000,
      includes: {
        sessions: true,
        archivedSessions: true,
        stateSqlite: true,
      },
      counts: {
        sessionFiles: 128,
        archivedSessionFiles: 14,
      },
    },
    entries: {
      sessions: true,
      archivedSessions: true,
      stateSqlite: true,
    },
  }),
  import_session_zip: (args) => {
    const mode = (args as { request?: { mode?: string } })?.request?.mode ?? "merge";
    return {
      mode,
      manifest: {
        version: 1,
        product: "CodexPilot",
        exportedAt: "1748073600000",
        exportedAtMs: 1748073600000,
        includes: {
          sessions: true,
          archivedSessions: true,
          stateSqlite: true,
        },
        counts: {
          sessionFiles: 128,
          archivedSessionFiles: 14,
        },
      },
      restoredSessionFiles: 128,
      restoredArchivedSessionFiles: 14,
      restoredStateSqlite: mode === "overwrite",
      safetyBackupZipPath:
        mode === "overwrite"
          ? "/Users/huanglin/.codex/backups_state/session-zip/codex-sessions-backup-1748077200.zip"
          : null,
      message:
        mode === "overwrite"
          ? "预览模式：已覆盖恢复 ZIP，并先创建了一份本地安全备份。"
          : "预览模式：已合并导入 ZIP，state_5.sqlite 保持不变。",
    };
  },
  collect_diagnostics: () => "预览模式：诊断快照已生成",
};

export async function mockBackend<T>(command: string, args?: unknown): Promise<T> {
  const handler = commandHandlers[command];
  if (!handler) {
    throw new Error(`Missing UI preview mock for command: ${command}`);
  }
  return handler(args) as T;
}
