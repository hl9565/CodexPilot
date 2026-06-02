export type BackendStatus = {
  status: string;
  version: string;
};

export type LaunchSnapshot = {
  appPath: string | null;
  requestedAppPath: string;
  debugPort: number;
  helperPort: number;
  autoLaunchOnOpen: boolean;
  autoSyncSessionsOnLaunch: boolean;
  ready: boolean;
  codexInstalled: boolean;
  state: string;
  actionKind: string;
  actionLabel: string;
  helperReachable: boolean;
  debugReachable: boolean;
  codexRunning: boolean;
  detail: string;
  commandPreview: string[];
};

export type ProviderSnapshot = {
  activeProvider: string;
  mode: RunMode;
  profile: string;
  source: string;
  authPath: string;
  configured: boolean;
  authenticated: boolean;
  accountLabel: string | null;
  routeLabel: string;
  statusMessage: string;
  degraded: boolean;
  officialSnapshotAvailable: boolean;
  backupSnapshotAvailable: boolean;
  profiles: ProviderProfile[];
  activeProfileId: string;
};

export type CcsProviderSnapshot = {
  dbPath: string;
  availableCount: number;
  importableCount: number;
  status: string;
  message: string;
};

export type ProviderProfile = {
  id: string;
  name: string;
  baseUrl: string;
  bearerToken: string;
  mode: ProviderProfileMode;
  upstreamProtocol: UpstreamProtocol;
  authenticatedBehavior: AuthenticatedBehavior;
};

export type RunMode = "official" | "hybridApi" | "api";
export type ProviderProfileMode = "hybridApi" | "api";
export type AuthenticatedBehavior = "relay" | "officialDirect";
export type UpstreamProtocol = "responses" | "chatCompletions" | "anthropicMessages";
export type Theme = "light" | "dark";

export const THEME_STORAGE_KEY = "codex-pilot-theme";

export type ProviderProfileSaveResponse = {
  id: string;
  message: string;
};

export type OfficialSnapshotImportResult = {
  message: string;
  provider: ProviderSnapshot;
};

export type OfficialSnapshotPrepareResult = {
  message: string;
  provider: ProviderSnapshot;
};

export type ProviderProfileSaveRequest = {
  id: string | null;
  name: string;
  baseUrl: string;
  bearerToken: string;
  mode: ProviderProfileMode;
  upstreamProtocol: UpstreamProtocol;
  authenticatedBehavior: AuthenticatedBehavior;
  activate: boolean;
};

export type CcsImportResult = {
  importedCount: number;
  skippedCount: number;
  renamedCount: number;
  provider: ProviderSnapshot;
  ccs: CcsProviderSnapshot;
  message: string;
};

export type ProviderCount = {
  provider: string;
  count: number;
};

export type ProviderSyncSnapshot = {
  targetProvider: string;
  currentProvider: string;
  availableProviders: string[];
  rolloutFiles: number;
  rolloutRewriteNeeded: number;
  sqliteRows: number;
  sqliteProviderRowsNeedingSync: number;
  sqliteTotalUpdatesNeeded: number;
  rolloutProviders: ProviderCount[];
  sqliteProviders: ProviderCount[];
};

export type DiagnosticCheck = {
  name: string;
  status: string;
  detail: string;
};

export type DiagnosticsSnapshot = {
  checks: DiagnosticCheck[];
  logs: string[];
};

export type RecycleBinEntry = {
  token: string;
  sessionId: string;
  title: string | null;
  projectCwd: string | null;
  schema: string;
  dbPath: string;
  backupPath: string;
  deletedAt: number | null;
  lastActiveAt: number | null;
  recoverable: boolean;
  status: string;
};

export type RecycleBinSnapshot = {
  entries: RecycleBinEntry[];
};

export type RecycleBinBatchResponse = {
  message: string;
  succeededTokens: string[];
  failed: Array<{
    token: string;
    message: string;
  }>;
};

export type SessionZipManifest = {
  version: number;
  product: string;
  exportedAt: string;
  exportedAtMs: number;
  includes: {
    sessions: boolean;
    archivedSessions: boolean;
    stateSqlite: boolean;
  };
  counts: {
    sessionFiles: number;
    archivedSessionFiles: number;
  };
};

export type SessionZipExportResult = {
  zipPath: string;
  manifest: SessionZipManifest;
};

export type SessionZipInspectResult = {
  zipPath: string;
  manifest: SessionZipManifest;
  entries: {
    sessions: boolean;
    archivedSessions: boolean;
    stateSqlite: boolean;
  };
};

export type SessionZipImportMode = "merge" | "overwrite";

export type SessionZipImportResult = {
  mode: SessionZipImportMode;
  manifest: SessionZipManifest;
  restoredSessionFiles: number;
  restoredArchivedSessionFiles: number;
  restoredStateSqlite: boolean;
  safetyBackupZipPath: string | null;
  message: string;
};

export type EnhancementSettings = {
  enabled: boolean;
  timeline: boolean;
  inlineActions: boolean;
  scrollRestore: boolean;
  pluginEntryUnlock: boolean;
  forcePluginInstall: boolean;
};

export type ViewId = "overview" | "launch" | "provider" | "sessions" | "diagnostics";
