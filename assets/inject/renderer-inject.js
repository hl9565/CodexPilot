(function () {
  const scriptVersion = "__CODEX_PILOT_VERSION__";
  const bootId = `${Date.now()}-${Math.random()}`;
  window.__CODEX_PILOT_BOOT_ID__ = bootId;
  if (window.__codexPilotBackendStatusHeartbeat) {
    window.clearInterval?.(window.__codexPilotBackendStatusHeartbeat);
    delete window.__codexPilotBackendStatusHeartbeat;
  }
  const existingRoot = document.getElementById("codex-pilot-root");
  if (existingRoot) {
    existingRoot.remove();
  }
  document.getElementById("codex-pilot-timeline")?.remove();
  document.querySelectorAll(".codex-pilot-row-actions, .codex-pilot-archive-action, .codex-pilot-toast").forEach((node) => node.remove());
  document.querySelectorAll("[data-codex-pilot-row], [data-codex-pilot-archive-row]").forEach((node) => {
    delete node.dataset.codexPilotRow;
    delete node.dataset.codexPilotArchiveRow;
  });
  window.__CODEX_PILOT_INJECTED__ = scriptVersion;

  const helperPort = Number("__CODEX_PILOT_HELPER_PORT__");
  const rootId = "codex-pilot-root";
  const timelineRootId = "codex-pilot-timeline";
  const actionGroupClass = "codex-pilot-row-actions";
  const actionButtonClass = "codex-pilot-row-action";
  const rowTitleMaskClass = "codex-pilot-row-title-mask-target";
  const archiveActionClass = "codex-pilot-archive-action";
  const scrollStoreKey = "codexPilotThreadScroll";
  const serviceTierStoreKey = "codexPilotThreadFastMode";
  const serviceTierStoreVersion = 1;
  const serviceTierDraftTtlMs = 60000;
  const fastServiceTierValue = "priority";
  const fastMaxEntries = 120;
  const fastDispatcherPatchVersion = "3";
  const maxScrollEntries = 100;
  const selectors = {
    sidebarThread: "[data-app-action-sidebar-thread-id]",
    threadTitle: "[data-thread-title], .truncate.select-none, .truncate.text-base",
    archiveNav: 'button[aria-label="已归档对话"], button[aria-label="Archived conversations"]',
    disabledInstallButton: 'button:disabled, button[aria-disabled="true"], [role="button"][aria-disabled="true"], button[data-disabled], [role="button"][data-disabled], button.cursor-not-allowed, [role="button"].cursor-not-allowed, button.pointer-events-none, [role="button"].pointer-events-none',
    pluginNavButton: 'nav[role="navigation"] button.h-token-nav-row.w-full',
    pluginSvgPath: 'svg path[d^="M7.94562 14.0277"]'
  };
  let lastUndoToken = null;
  let activeScrollSessionId = "";
  let restoreInProgressUntil = 0;
  let userScrollIntentUntil = 0;
  let scrollSaveTimer = null;
  let routeCheckTimer = null;
  let lastTimelineSignature = "";
  let lastTimelineNoTargetsAt = 0;
  let consecutiveBackendTimeouts = 0;
  let recoveryInFlight = false;
  let lastRecoveryAttemptAt = 0;
  let activeFastSessionId = "";
  let fastToggleButton = null;
  let fastToggleLabel = null;
  let fastPendingBindStartedAt = 0;
  let fastPendingBindStartToken = "";
  let fastModuleLoadPromise = null;
  let fastDispatcherPatchInstalled = false;
  let fastDispatcherPatchPromise = null;
  let fastDispatcherPatchStatus = "loading";
  let routeTrackingInstalled = false;
  const backendStatusTimeoutMs = 2000;
  const backendRecoveryThreshold = 3;
  const backendRecoveryCooldownMs = 60000;
  const defaultEnhancementSettings = {
    enabled: true,
    timeline: true,
    inlineActions: true,
    scrollRestore: true,
    pluginEntryUnlock: true,
    forcePluginInstall: true,
    fastGlobalMode: true
  };
  let enhancementSettings = { ...defaultEnhancementSettings };

  function codexAppAssetUrl(namePart) {
    const urls = [
      ...Array.from(document.scripts || []).map((script) => script.src),
      ...Array.from(document.querySelectorAll("link[href]") || []).map((link) => link.href),
      ...performance.getEntriesByType("resource").map((entry) => entry.name),
    ].filter(Boolean);
    return urls.find((url) => url.includes("/assets/") && url.includes(namePart) && url.split("?")[0].endsWith(".js")) || "";
  }

  async function loadCodexAppModule(namePart) {
    if (!fastModuleLoadPromise || fastModuleLoadPromise.namePart !== namePart) {
      const promise = Promise.resolve().then(async () => {
        if (window.__CODEX_PILOT_TEST__ && typeof window.__CODEX_PILOT_TEST_LOAD_CODEX_APP_MODULE__ === "function") {
          return await window.__CODEX_PILOT_TEST_LOAD_CODEX_APP_MODULE__(namePart);
        }
        const url = codexAppAssetUrl(namePart);
        if (!url) throw new Error(`未找到 Codex App asset: ${namePart}`);
        return await import(url);
      }).catch((error) => {
        if (fastModuleLoadPromise === promise) {
          fastModuleLoadPromise = null;
        }
        throw error;
      });
      promise.namePart = namePart;
      fastModuleLoadPromise = promise;
    }
    return await fastModuleLoadPromise;
  }

  function isActiveBoot() {
    return window.__CODEX_PILOT_BOOT_ID__ === bootId;
  }

  window.__CODEX_PILOT__ = {
    version: scriptVersion,
    helperPort,
    backendUrl: `http://127.0.0.1:${helperPort}`,
    bridge(path, payload = {}) {
      if (typeof window.__codexPilotBridge === "function") {
        return window.__codexPilotBridge(path, payload);
      }
      return Promise.resolve({
        status: "failed",
        message: "CodexPilot 桥接不可用"
      });
    },
    backendStatus() {
      return this.bridge("/backend/status");
    },
    recoverBridge() {
      return this.bridge("/backend/recover-bridge");
    },
    enhancementSettings() {
      return this.bridge("/enhancement/settings");
    },
    detectSession() {
      return detectCurrentSession();
    },
    exportMarkdown(session) {
      return this.bridge("/session/export-markdown", session);
    },
    exportHtml(session) {
      return this.bridge("/session/export-html", session);
    },
    deleteSession(session) {
      return this.bridge("/session/delete", session);
    },
    undoDelete(undoToken) {
      return this.bridge("/session/undo", { undo_token: undoToken });
    },
    findArchivedThread(title) {
      return this.bridge("/session/archived-thread", { title });
    },
    report(event, detail = {}) {
      return this.bridge("/diagnostics/report", { event, detail });
    }
  };

  function reportRendererEvent(event, detail = {}) {
    try {
      const report = window.__CODEX_PILOT__.report(event, {
        ...detail,
        href: String(window.location.href || ""),
        title: document.title || ""
      });
      if (report && typeof report.catch === "function") {
        report.catch(() => {});
      }
    } catch (_error) {
      // Diagnostic reporting must never break the Codex page.
    }
  }

  function normalizeEnhancementSettings(value) {
    const source = value && typeof value === "object" ? value : {};
    return {
      enabled: source.enabled !== false,
      timeline: source.timeline !== false,
      inlineActions: source.inlineActions !== false,
      scrollRestore: source.scrollRestore !== false,
      pluginEntryUnlock: source.pluginEntryUnlock !== false,
      forcePluginInstall: source.forcePluginInstall !== false,
      fastGlobalMode: source.fastGlobalMode !== false
    };
  }

  async function loadEnhancementSettings() {
    try {
      const response = await window.__CODEX_PILOT__.enhancementSettings();
      const result = response.result || response;
      enhancementSettings = normalizeEnhancementSettings(result);
    } catch (error) {
      enhancementSettings = { ...defaultEnhancementSettings };
      reportRendererEvent("enhancement_settings_error", { message: String(error) });
    }
    return enhancementSettings;
  }

  function applyPluginPatches() {
    if (enhancementSettings.pluginEntryUnlock) {
      enablePluginEntry();
    } else {
      clearPluginPatchArtifacts();
    }
    unblockPluginInstallButtons();
  }

  function ensureStyles() {
    const style = document.getElementById("codex-pilot-style") || document.createElement("style");
    style.id = "codex-pilot-style";
    style.dataset.codexPilotVersion = scriptVersion;
    style.textContent = `
      #${rootId} {
        bottom: 18px;
        color: #1d2630;
        font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        position: fixed;
        right: 18px;
        z-index: 2147483647;
      }

      #${rootId} * {
        box-sizing: border-box;
      }

      #${rootId} .codex-pilot-button {
        align-items: center;
        background: rgba(255, 255, 255, 0.86);
        border: 1px solid rgba(148, 163, 184, 0.42);
        border-radius: 999px;
        box-shadow: 0 8px 20px rgba(15, 23, 42, 0.12);
        color: #263241;
        cursor: pointer;
        display: inline-flex;
        font-size: 13px;
        font-weight: 700;
        gap: 7px;
        min-height: 30px;
        padding: 0 10px;
        transition: background 120ms ease, border-color 120ms ease, box-shadow 120ms ease;
      }

      #${rootId} .codex-pilot-pill {
        align-items: center;
        display: inline-flex;
        gap: 6px;
      }

      #${rootId} .codex-pilot-fast-toggle {
        align-items: center;
        background: rgba(255, 255, 255, 0.86);
        border: 1px solid rgba(148, 163, 184, 0.42);
        border-radius: 999px;
        box-shadow: 0 8px 20px rgba(15, 23, 42, 0.12);
        color: #94a3b8;
        cursor: pointer;
        display: inline-flex;
        font: inherit;
        font-size: 11px;
        font-weight: 700;
        gap: 4px;
        min-height: 30px;
        min-width: 30px;
        padding: 0 8px;
        transition: background 120ms ease, border-color 120ms ease, box-shadow 120ms ease, color 120ms ease;
      }

      #${rootId} .codex-pilot-fast-toggle:hover {
        background: rgba(255, 255, 255, 0.96);
        border-color: rgba(100, 116, 139, 0.55);
        box-shadow: 0 10px 24px rgba(15, 23, 42, 0.16);
      }

      #${rootId} .codex-pilot-fast-toggle[data-mode="fast"] {
        color: #d6a21d;
      }

      #${rootId} .codex-pilot-fast-toggle[aria-disabled="true"] {
        cursor: help;
        opacity: 0.72;
      }

      #${rootId} .codex-pilot-fast-toggle svg {
        display: block;
        height: 14px;
        width: 14px;
      }

      #${rootId} .codex-pilot-fast-label {
        line-height: 1;
        overflow: hidden;
        white-space: nowrap;
      }

      #${rootId} .codex-pilot-button:hover,
      #${rootId}[data-open="true"] .codex-pilot-button {
        background: rgba(255, 255, 255, 0.96);
        border-color: rgba(100, 116, 139, 0.55);
        box-shadow: 0 10px 24px rgba(15, 23, 42, 0.16);
      }

      #${rootId} .codex-pilot-status-dot {
        background: #94a3b8;
        border-radius: 999px;
        display: inline-block;
        height: 7px;
        width: 7px;
      }

      #${rootId}[data-status="checking"] .codex-pilot-status-dot {
        background: #d6a21d;
      }

      #${rootId}[data-status="connected"] .codex-pilot-status-dot {
        background: #16a36a;
      }

      #${rootId} .codex-pilot-panel {
        background: #ffffff;
        border: 1px solid #d7dde5;
        border-radius: 8px;
        bottom: 48px;
        box-shadow: 0 18px 42px rgba(15, 23, 42, 0.22);
        display: none;
        min-width: 260px;
        padding: 10px;
        position: absolute;
        right: 0;
      }

      #${rootId}[data-open="true"] .codex-pilot-panel {
        display: block;
      }

      #${rootId} .codex-pilot-title {
        align-items: center;
        display: flex;
        justify-content: space-between;
        margin-bottom: 8px;
      }

      #${rootId} .codex-pilot-title strong {
        font-size: 13px;
      }

      #${rootId} .codex-pilot-version {
        color: #6b7788;
        font-size: 11px;
        font-weight: 700;
      }

      #${rootId} .codex-pilot-action {
        align-items: center;
        background: #f7f9fc;
        border: 1px solid #e0e6ef;
        border-radius: 7px;
        color: #233044;
        cursor: pointer;
        display: flex;
        font-size: 13px;
        justify-content: space-between;
        min-height: 34px;
        padding: 0 10px;
        width: 100%;
      }

      #${rootId} .codex-pilot-action + .codex-pilot-action {
        margin-top: 6px;
      }

      #${rootId} .codex-pilot-action:hover {
        background: #eef4ff;
        border-color: #c8d8fb;
      }

      #${rootId} .codex-pilot-export-shell {
        background: #eef2f7;
        border: 1px solid #d6dee8;
        border-radius: 10px;
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.78);
        padding: 4px;
      }

      #${rootId} .codex-pilot-export-split {
        background: #ffffff;
        border: 1px solid #cfd8e3;
        border-radius: 7px;
        display: grid;
        grid-template-columns: 1fr 1fr;
        overflow: hidden;
        padding: 2px;
      }

      #${rootId} .codex-pilot-export-option {
        align-items: center;
        background: #ffffff;
        border: 0;
        border-radius: 5px;
        color: #233044;
        cursor: pointer;
        display: flex;
        font: inherit;
        font-size: 13px;
        font-weight: 700;
        gap: 6px;
        justify-content: center;
        min-height: 32px;
        min-width: 0;
        padding: 0 8px;
        white-space: nowrap;
      }

      #${rootId} .codex-pilot-export-option + .codex-pilot-export-option {
        border-left: 1px solid #e0e6ef;
      }

      #${rootId} .codex-pilot-export-option:hover {
        background: #f8fbff;
        color: #1f3654;
      }

      #${rootId} .codex-pilot-export-option:disabled {
        cursor: not-allowed;
        opacity: 0.55;
      }

      #${rootId} .codex-pilot-export-option svg {
        flex: 0 0 auto;
        display: block;
        height: 14px;
        width: 14px;
      }

      #${rootId} .codex-pilot-export-option span {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      #${rootId} .codex-pilot-action[data-danger="true"] {
        background: #fff5f5;
        border-color: #ffd0d0;
        color: #9f1d1d;
      }

      #${rootId} .codex-pilot-action[data-danger="true"]:hover {
        background: #ffecec;
        border-color: #ffb9b9;
      }

      #${rootId} .codex-pilot-action:disabled {
        cursor: not-allowed;
        opacity: 0.55;
      }

      #${rootId} .codex-pilot-message {
        color: #5e6d7e;
        font-size: 12px;
        line-height: 1.45;
        margin-top: 8px;
        overflow-wrap: anywhere;
      }

      .codex-pilot-deleted-session {
        opacity: 0.44 !important;
        pointer-events: none !important;
        text-decoration: line-through !important;
      }

      [data-codex-pilot-row="true"] {
        position: relative !important;
      }

      .${actionGroupClass} {
        align-items: center;
        background: transparent;
        border: 0;
        box-shadow: none;
        display: inline-flex;
        gap: 6px;
        opacity: 0;
        pointer-events: none;
        position: absolute;
        left: 8px;
        top: 50%;
        padding: 0;
        transform: translateY(-50%);
        transition: opacity 120ms ease;
        z-index: 20;
      }

      [data-codex-pilot-row="true"]:hover .${actionGroupClass},
      [data-codex-pilot-row="true"]:focus-within .${actionGroupClass} {
        opacity: 1;
        pointer-events: auto;
      }

      [data-codex-pilot-row="true"]:hover .${rowTitleMaskClass},
      [data-codex-pilot-row="true"]:focus-within .${rowTitleMaskClass} {
        -webkit-mask-image: linear-gradient(90deg, transparent 0, transparent 42px, #000 58px, #000 100%);
        mask-image: linear-gradient(90deg, transparent 0, transparent 42px, #000 58px, #000 100%);
      }

      .${actionButtonClass},
      .${archiveActionClass} {
        align-items: center;
        background: rgba(31, 36, 48, 0.78);
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 5px;
        box-shadow: 0 5px 12px rgba(0, 0, 0, 0.16);
        color: #d9dee8;
        cursor: pointer;
        display: inline-flex;
        height: 26px;
        justify-content: center;
        padding: 0;
        width: 26px;
      }

      .${actionButtonClass}:hover,
      .${archiveActionClass}:hover {
        background: rgba(255, 255, 255, 0.12);
        color: #ffffff;
      }

      .${actionButtonClass}[data-danger="true"],
      .${archiveActionClass}[data-danger="true"] {
        color: #f7b4b4;
      }

      .${actionButtonClass}[data-danger="true"]:hover,
      .${archiveActionClass}[data-danger="true"]:hover {
        color: #ffffff;
      }

      .${archiveActionClass}.codex-pilot-archive-bar {
        background: rgba(127, 29, 29, 0.92);
        border: 1px solid rgba(255, 210, 210, 0.2);
        color: #ffffff;
        font-size: 12px;
        font-weight: 700;
        height: auto;
        min-height: 30px;
        padding: 0 10px;
        width: auto;
      }

      .${archiveActionClass}.codex-pilot-archive-bar:hover {
        background: rgba(153, 27, 27, 0.96);
      }

      .${actionButtonClass} svg,
      .${archiveActionClass} svg {
        display: block;
        height: 15px;
        pointer-events: none;
        width: 15px;
      }

      .codex-pilot-toast {
        background: rgba(17, 24, 39, 0.94);
        border-radius: 8px;
        bottom: 18px;
        color: #ffffff;
        font-size: 13px;
        left: 50%;
        max-width: min(560px, calc(100vw - 32px));
        padding: 10px 12px;
        position: fixed;
        transform: translateX(-50%);
        z-index: 2147483646;
      }

      .codex-pilot-force-install-unlocked {
        border-color: #ef4444 !important;
        background: #fee2e2 !important;
        color: #991b1b !important;
        opacity: 1 !important;
      }

      .codex-pilot-toast button {
        background: transparent;
        border: 0;
        color: #bfdbfe;
        cursor: pointer;
        font: inherit;
        font-weight: 700;
        margin-left: 10px;
        padding: 0;
      }

      .codex-pilot-archive-bar {
        margin: 8px 0 0 0;
      }

      .codex-pilot-timeline {
        bottom: 92px;
        pointer-events: none;
        position: fixed;
        right: 8px;
        top: 118px;
        width: 26px;
        z-index: 2147483000;
      }

      .codex-pilot-timeline-track {
        background: rgba(95, 107, 128, 0.28);
        border-radius: 999px;
        bottom: 6px;
        left: 12px;
        position: absolute;
        top: 6px;
        width: 2px;
      }

      .codex-pilot-timeline-marker {
        align-items: center;
        background: #ffffff;
        border: 1px solid rgba(37, 99, 235, 0.54);
        border-radius: 999px;
        box-shadow: 0 2px 8px rgba(15, 23, 42, 0.18);
        color: #2563eb;
        cursor: pointer;
        display: flex;
        height: 11px;
        justify-content: center;
        left: 8px;
        padding: 0;
        pointer-events: auto;
        position: absolute;
        transform: translateY(-50%);
        width: 11px;
      }

      .codex-pilot-timeline-marker:hover,
      .codex-pilot-timeline-marker:focus-visible {
        background: #2563eb;
        border-color: #ffffff;
        outline: none;
      }

      .codex-pilot-timeline-tooltip {
        background: rgba(17, 24, 39, 0.94);
        border-radius: 7px;
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.22);
        color: #ffffff;
        display: none;
        font-size: 12px;
        line-height: 1.35;
        max-width: min(280px, calc(100vw - 72px));
        padding: 7px 9px;
        position: absolute;
        right: 22px;
        top: 50%;
        transform: translateY(-50%);
        white-space: normal;
        width: max-content;
      }

      .codex-pilot-timeline-marker:hover .codex-pilot-timeline-tooltip,
      .codex-pilot-timeline-marker:focus-visible .codex-pilot-timeline-tooltip {
        display: block;
      }
    `;
    if (!style.parentElement) {
      document.head.appendChild(style);
    }
  }

  function detectCurrentSession() {
    const byUrl = sessionRefFromUrl();
    const bySelectedRow = sessionRefFromSelectedRow();
    const byVisibleRow = sessionRefFromVisibleRows(byUrl?.session_id);
    return bySelectedRow || byVisibleRow || byUrl || null;
  }

  function normalizeServiceTierMode(mode) {
    const normalized = String(mode || "").trim().toLowerCase();
    return normalized === "fast" ? "fast" : "standard";
  }

  function validServiceTierSessionKey(sessionId) {
    const key = String(sessionId || "").trim();
    if (!key || key === "__proto__" || key === "prototype" || key === "constructor") return "";
    return /^[A-Za-z0-9_.-]{8,128}$/.test(key) ? key : "";
  }

  function validServiceTierPathSessionKey(sessionId) {
    const key = validServiceTierSessionKey(sessionId);
    if (!key) return "";
    if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(key)) return key;
    if (/^thread-[A-Za-z0-9_.-]{8,}$/i.test(key)) return key;
    return "";
  }

  function serviceTierSessionIdsInSidebar() {
    const ids = Array.from(document.querySelectorAll(selectors.sidebarThread))
      .map((row) => validServiceTierSessionKey(row.getAttribute("data-app-action-sidebar-thread-id")));
    try {
      ids.push(...Object.keys(readScrollStore() || {}).map(validServiceTierSessionKey));
    } catch (_error) {}
    return Array.from(new Set(ids.filter(Boolean))).slice(0, fastMaxEntries);
  }

  function isCodexDraftRoute() {
    try {
      const url = new URL(window.location.href || "");
      const pathname = url.pathname.replace(/\/+$/, "");
      return pathname === "/codex" || pathname.endsWith("/codex/new");
    } catch (_error) {
      return false;
    }
  }

  function currentServiceTierSessionKey() {
    const byUrl = validServiceTierSessionKey(sessionRefFromUrl()?.session_id);
    if (byUrl) return byUrl;
    return "";
  }

  function currentServiceTierBindSessionKey() {
    return validServiceTierSessionKey(sessionRefFromUrl()?.session_id);
  }

  function isFastMode(mode) {
    return normalizeServiceTierMode(mode) === "fast";
  }

  function readServiceTierStore() {
    try {
      const parsed = JSON.parse(window.localStorage?.getItem(serviceTierStoreKey) || "{}");
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        return { version: serviceTierStoreVersion, entries: {}, draft: null };
      }
      const rawEntries = parsed.version === serviceTierStoreVersion && parsed.entries && typeof parsed.entries === "object"
        ? parsed.entries
        : {};
      const entries = Object.create(null);
      Object.entries(rawEntries).forEach(([key, value]) => {
        const sessionId = validServiceTierSessionKey(key);
        if (!sessionId) return;
        const mode = normalizeServiceTierMode(value?.mode);
        entries[sessionId] = {
          mode,
          at: finiteScrollNumber(value?.at) || Date.now()
        };
      });
      const draftRaw = parsed.version === serviceTierStoreVersion ? parsed.draft : null;
      const draft = draftRaw && typeof draftRaw === "object"
        ? {
            mode: normalizeServiceTierMode(draftRaw.mode),
            at: finiteScrollNumber(draftRaw.at) || Date.now(),
            pendingBind: draftRaw.pendingBind === true,
            startToken: String(draftRaw.startToken || ""),
            previousSessionId: validServiceTierSessionKey(draftRaw.previousSessionId),
            existingSessionIds: Array.isArray(draftRaw.existingSessionIds)
              ? draftRaw.existingSessionIds.map(validServiceTierSessionKey).filter(Boolean).slice(0, fastMaxEntries)
              : []
          }
        : null;
      if (draft && Date.now() - draft.at > serviceTierDraftTtlMs) {
        return { version: serviceTierStoreVersion, entries, draft: null };
      }
      return { version: serviceTierStoreVersion, entries, draft };
    } catch (error) {
      reportRendererEvent("thread_fast_store_read_error", { message: String(error) });
      return { version: serviceTierStoreVersion, entries: {}, draft: null };
    }
  }

  function writeServiceTierStore(store) {
    try {
      const payload = {
        version: serviceTierStoreVersion,
        entries: Object.fromEntries(
          Object.entries(store?.entries || {})
            .map(([key, value]) => [validServiceTierSessionKey(key), value])
            .filter(([key, value]) => key && value && typeof value === "object")
            .map(([key, value]) => [
              key,
              {
                mode: normalizeServiceTierMode(value.mode),
                at: finiteScrollNumber(value.at) || Date.now()
              }
            ])
            .sort((left, right) => right[1].at - left[1].at)
            .slice(0, fastMaxEntries)
        ),
        draft: store?.draft
          ? {
              mode: normalizeServiceTierMode(store.draft.mode),
              at: finiteScrollNumber(store.draft.at) || Date.now(),
              pendingBind: store.draft.pendingBind === true,
              startToken: String(store.draft.startToken || ""),
              previousSessionId: validServiceTierSessionKey(store.draft.previousSessionId),
              existingSessionIds: Array.isArray(store.draft.existingSessionIds)
                ? store.draft.existingSessionIds.map(validServiceTierSessionKey).filter(Boolean).slice(0, fastMaxEntries)
                : []
            }
          : null
      };
      window.localStorage?.setItem(serviceTierStoreKey, JSON.stringify(payload));
      return true;
    } catch (error) {
      reportRendererEvent("thread_fast_store_write_error", { message: String(error) });
      return false;
    }
  }

  function currentServiceTierThreadOverride(sessionId = currentServiceTierSessionKey()) {
    const normalizedSessionId = validServiceTierSessionKey(sessionId);
    if (!normalizedSessionId) return null;
    const entry = readServiceTierStore().entries[normalizedSessionId];
    if (!entry) return null;
    return {
      mode: normalizeServiceTierMode(entry.mode),
      at: finiteScrollNumber(entry.at)
    };
  }

  function currentServiceTierDraft() {
    const draft = readServiceTierStore().draft;
    if (!draft) return null;
    if (Date.now() - draft.at > serviceTierDraftTtlMs) {
      clearServiceTierDraft("expired");
      return null;
    }
    return {
      mode: normalizeServiceTierMode(draft.mode),
      at: finiteScrollNumber(draft.at),
      pendingBind: draft.pendingBind === true,
      startToken: String(draft.startToken || ""),
      previousSessionId: validServiceTierSessionKey(draft.previousSessionId),
      existingSessionIds: Array.isArray(draft.existingSessionIds)
        ? draft.existingSessionIds.map(validServiceTierSessionKey).filter(Boolean).slice(0, fastMaxEntries)
        : []
    };
  }

  function setServiceTierThreadOverride(sessionId, mode) {
    const normalizedSessionId = validServiceTierSessionKey(sessionId);
    if (!normalizedSessionId) return false;
    const store = readServiceTierStore();
    store.entries[normalizedSessionId] = {
      mode: normalizeServiceTierMode(mode),
      at: Date.now()
    };
    return writeServiceTierStore(store);
  }

  function setServiceTierDraft(
    mode,
    pendingBind = false,
    startToken = "",
    previousSessionId = currentServiceTierSessionKey(),
    existingSessionIds = serviceTierSessionIdsInSidebar()
  ) {
    const store = readServiceTierStore();
    store.draft = {
      mode: normalizeServiceTierMode(mode),
      at: Date.now(),
      pendingBind,
      startToken: String(startToken || ""),
      previousSessionId: validServiceTierSessionKey(previousSessionId),
      existingSessionIds: Array.isArray(existingSessionIds)
        ? existingSessionIds.map(validServiceTierSessionKey).filter(Boolean).slice(0, fastMaxEntries)
        : []
    };
    return writeServiceTierStore(store);
  }

  function clearServiceTierDraft(reason = "cleared") {
    const store = readServiceTierStore();
    if (!store.draft) return false;
    store.draft = null;
    const wrote = writeServiceTierStore(store);
    if (wrote) {
      reportRendererEvent("thread_fast_draft_cleared", { reason });
    }
    return wrote;
  }

  function markServiceTierDraftPendingBind(startToken = fastPendingBindStartToken || "") {
    const store = readServiceTierStore();
    if (!store.draft) return false;
    if (store.draft.pendingBind) {
      return true;
    }
    store.draft.pendingBind = true;
    store.draft.at = Date.now();
    store.draft.startToken = String(startToken || "");
    store.draft.previousSessionId = validServiceTierSessionKey(store.draft.previousSessionId || currentServiceTierSessionKey());
    store.draft.existingSessionIds = serviceTierSessionIdsInSidebar();
    return writeServiceTierStore(store);
  }

  function currentServiceTierUiState() {
    const sessionId = currentServiceTierSessionKey();
    if (enhancementSettings.fastGlobalMode) {
      return {
        source: "global",
        sessionId,
        mode: "fast",
        pendingBind: false
      };
    }
    const threadOverride = currentServiceTierThreadOverride(sessionId);
    if (threadOverride) {
      return {
        source: "thread",
        sessionId,
        mode: threadOverride.mode,
        pendingBind: false
      };
    }
    if (!sessionId) {
      const draft = currentServiceTierDraft();
      if (draft) {
        return {
          source: "draft",
          sessionId: "",
          mode: draft.mode,
          pendingBind: draft.pendingBind
        };
      }
    }
    return {
      source: "none",
      sessionId,
      mode: "standard",
      pendingBind: false
    };
  }

  function serviceTierDraftCanApplyToThreadStart(draft, threadId) {
    if (!draft) return false;
    const normalizedThreadId = validServiceTierSessionKey(threadId);
    if (!normalizedThreadId) return true;
    if (draft.previousSessionId && draft.previousSessionId === normalizedThreadId) return false;
    if (draft.existingSessionIds?.includes(normalizedThreadId)) return false;
    const currentSessionId = currentServiceTierSessionKey();
    if (!draft.pendingBind && currentSessionId && currentSessionId === normalizedThreadId) return false;
    return true;
  }

  function currentServiceTierDraftForThreadStart(threadId = "") {
    const draft = currentServiceTierDraft();
    if (!serviceTierDraftCanApplyToThreadStart(draft, threadId)) return null;
    return draft;
  }

  function handleServiceTierSessionMaybeChanged(reason = "poll") {
    const bindSessionId = currentServiceTierBindSessionKey();
    if (bindSessionId) {
      maybeBindDraftToCurrentSession(reason, bindSessionId);
    }
    refreshFastToggleUi();
  }

  function handleRouteMaybeChanged(reason = "poll") {
    handleServiceTierSessionMaybeChanged(reason);
    if (enhancementSettings.scrollRestore) {
      handleThreadMaybeChanged(reason);
    }
  }

  function installRouteChangeTracking() {
    if (!enhancementSettings.enabled || routeTrackingInstalled) return;
    routeTrackingInstalled = true;
    handleServiceTierSessionMaybeChanged("route_tracking_ready");
    ["pushState", "replaceState"].forEach((method) => {
      const originalKey = `__codexPilotOriginal${method}`;
      const original = window[originalKey] || history?.[method];
      if (typeof original !== "function") return;
      window[originalKey] = original;
      history[method] = function codexPilotPatchedHistory(...args) {
        if (!isActiveBoot()) {
          return original.apply(this, args);
        }
        if (enhancementSettings.scrollRestore) {
          saveThreadScrollPosition(`before_${method}`);
        }
        const result = original.apply(this, args);
        setTimeout(() => handleRouteMaybeChanged(method), 0);
        return result;
      };
    });
    if (!window.__codexPilotPopstatePatched) {
      window.__codexPilotPopstatePatched = true;
      window.addEventListener?.("popstate", () => {
        if (!window.__CODEX_PILOT_ACTIVE_POPSTATE_HANDLER__) return;
        window.__CODEX_PILOT_ACTIVE_POPSTATE_HANDLER__();
      }, true);
    }
    window.__CODEX_PILOT_ACTIVE_POPSTATE_HANDLER__ = () => {
      if (!isActiveBoot()) return;
      if (enhancementSettings.scrollRestore) {
        saveThreadScrollPosition("before_popstate");
      }
      setTimeout(() => handleRouteMaybeChanged("popstate"), 0);
    };
    if (typeof window.setInterval === "function") {
      routeCheckTimer = window.setInterval(() => {
        if (!isActiveBoot()) return;
        handleRouteMaybeChanged("interval");
      }, 650);
    }
  }

  function currentServiceTierTooltip(state = currentServiceTierUiState()) {
    if (state.source === "global") {
      return "Pilot 已启用，所有对话使用 Fast";
    }
    if (state.source === "draft") {
      return state.mode === "fast"
        ? "下一条新对话将使用 Fast"
        : "下一条新对话将使用 Standard";
    }
    return state.mode === "fast"
      ? "当前对话使用 Fast"
      : "当前对话使用 Standard";
  }

  function currentServiceTierToast(state = currentServiceTierUiState()) {
    if (state.source === "global") {
      return "Pilot 已启用，所有对话使用 Fast";
    }
    if (state.source === "draft") {
      return state.mode === "fast"
        ? "下一条新对话将使用 Fast"
        : "下一条新对话将使用 Standard";
    }
    return state.mode === "fast"
      ? "当前对话已切换为 Fast"
      : "当前对话已切换为 Standard";
  }

  function refreshFastToggleUi() {
    const state = currentServiceTierUiState();
    activeFastSessionId = state.source === "thread" ? state.sessionId : "";
    if (fastToggleButton) {
      fastToggleButton.dataset.mode = state.mode;
      fastToggleButton.dataset.source = state.source;
      fastToggleButton.dataset.patchStatus = fastDispatcherPatchStatus;
      const disabled = fastDispatcherPatchStatus !== "ready";
      fastToggleButton.disabled = false;
      if (disabled) {
        fastToggleButton.setAttribute("aria-disabled", "true");
      } else {
        fastToggleButton.removeAttribute("aria-disabled");
      }
      const label = disabled ? "Fast 暂不可用，正在连接 Codex 请求通道" : currentServiceTierTooltip(state);
      fastToggleButton.title = label;
      fastToggleButton.setAttribute("aria-label", label);
    }
    if (fastToggleLabel) {
      fastToggleLabel.textContent = state.mode === "fast" ? "Fast" : "";
    }
  }

  function toggleCurrentServiceTierMode() {
    if (fastDispatcherPatchStatus !== "ready") {
      showToast("Fast 暂不可用，正在连接 Codex 请求通道", null);
      reportRendererEvent("thread_fast_toggle_blocked", { patch_status: fastDispatcherPatchStatus });
      refreshFastToggleUi();
      return;
    }
    const state = currentServiceTierUiState();
    if (enhancementSettings.fastGlobalMode) {
      refreshFastToggleUi();
      showToast(currentServiceTierToast(state), null);
      reportRendererEvent("thread_fast_global_active", {
        source: state.source,
        session_id: state.sessionId,
        mode: state.mode
      });
      return;
    }
    const nextMode = state.mode === "fast" ? "standard" : "fast";
    const sessionId = currentServiceTierSessionKey();
    let wrote = false;
    if (sessionId) {
      wrote = setServiceTierThreadOverride(sessionId, nextMode);
      clearServiceTierDraft("replaced_by_thread_toggle");
    } else if (nextMode === "standard") {
      wrote = clearServiceTierDraft("draft_toggled_standard") || true;
    } else {
      wrote = setServiceTierDraft(nextMode, false);
    }
    if (wrote) {
      refreshFastToggleUi();
      showToast(currentServiceTierToast({
        source: sessionId ? "thread" : "draft",
        sessionId,
        mode: nextMode
      }), null);
      reportRendererEvent("thread_fast_toggled", {
        source: sessionId ? "thread" : "draft",
        session_id: sessionId,
        mode: nextMode
      });
    }
  }

  function sessionPayload(session) {
    return {
      id: session.session_id,
      session_id: session.session_id,
      title: session.title || ""
    };
  }

  function sessionRefFromUrl() {
    const href = String(window.location.href || "");
    const sessionId = sessionIdFromUrl(href);
    return sessionId ? { session_id: sessionId, title: document.title || "当前会话", source: "url" } : null;
  }

  function sessionIdFromUrl(href) {
    try {
      const url = new URL(href);
      for (const key of ["session", "conversation", "thread"]) {
        const value = validServiceTierSessionKey(url.searchParams.get(key));
        if (value) return value;
      }
      const hashQueryIndex = url.hash.indexOf("?");
      if (hashQueryIndex >= 0) {
        const hashParams = new URLSearchParams(url.hash.slice(hashQueryIndex + 1));
        for (const key of ["session", "conversation", "thread"]) {
          const value = validServiceTierSessionKey(hashParams.get(key));
          if (value) return value;
        }
      }
      const segments = url.pathname.split("/").map((segment) => segment.trim()).filter(Boolean);
      for (let index = segments.length - 1; index >= 0; index -= 1) {
        const decoded = decodeURIComponent(segments[index]);
        const previous = index > 0 ? decodeURIComponent(segments[index - 1]) : "";
        const explicitSessionContext = /^(session|sessions|conversation|conversations|thread|threads)$/i.test(previous);
        const value = explicitSessionContext
          ? validServiceTierSessionKey(decoded)
          : validServiceTierPathSessionKey(decoded);
        if (value) return value;
      }
    } catch (_error) {
      const match = String(href || "").match(/(?:[?&#]|^)(?:session|conversation|thread)=([A-Za-z0-9_.-]{8,})/i);
      return validServiceTierSessionKey(match?.[1]);
    }
    return "";
  }

  function sessionRefFromSelectedRow() {
    const rows = Array.from(document.querySelectorAll(selectors.sidebarThread));
    const selected = rows.find((row) => {
      const aria = row.getAttribute("aria-current") || row.getAttribute("aria-selected");
      if (aria === "true" || aria === "page") return true;
      const className = String(row.className || "");
      if (/\b(active|selected)\b/i.test(className)) return true;
      return row.matches?.("[data-active='true'], [data-selected='true']");
    });
    return selected ? sessionRefFromRow(selected, "selected-row") : null;
  }

  function sessionRefFromVisibleRows(preferredId) {
    const rows = Array.from(document.querySelectorAll(selectors.sidebarThread));
    if (!rows.length) return null;
    if (preferredId) {
      const matched = rows.find((row) => row.getAttribute("data-app-action-sidebar-thread-id") === preferredId);
      if (matched) return sessionRefFromRow(matched, "matched-url-row");
    }
    const visible = rows.find((row) => {
      const rect = row.getBoundingClientRect?.();
      return rect && rect.width > 0 && rect.height > 0;
    });
    return visible ? sessionRefFromRow(visible, "first-visible-row") : null;
  }

  function sessionRefFromRow(row, source) {
    const sessionId = row.getAttribute("data-app-action-sidebar-thread-id") || "";
    if (!sessionId) return null;
    const titleNode = row.querySelector(selectors.threadTitle);
    const title = normalizeText(titleNode?.textContent || row.textContent || "未命名会话");
    return { session_id: sessionId, title, source };
  }

  function rowForSession(sessionId) {
    if (!sessionId) return null;
    const rows = Array.from(document.querySelectorAll(selectors.sidebarThread));
    return rows.find((row) => row.getAttribute("data-app-action-sidebar-thread-id") === sessionId) || null;
  }

  function sessionRows() {
    return Array.from(document.querySelectorAll(selectors.sidebarThread)).filter((row) => {
      const rect = row.getBoundingClientRect?.();
      return row.getAttribute("data-app-action-sidebar-thread-id") && (!rect || (rect.width > 0 && rect.height > 0));
    });
  }

  function removableRowContainer(row) {
    if (!row) return null;
    const candidates = [
      row.closest("[role='listitem']"),
      row.closest("li"),
      row.closest("[data-testid*='thread']"),
      row
    ];
    return candidates.find((candidate) => candidate && candidate.parentElement) || null;
  }

  function syncDeletedSessionRow(session) {
    const row = rowForSession(session?.session_id);
    if (!row) return false;
    const container = removableRowContainer(row);
    if (!container) return false;
    try {
      container.remove();
      return true;
    } catch (_error) {
      row.classList.add("codex-pilot-deleted-session");
      row.setAttribute("aria-disabled", "true");
      return true;
    }
  }

  function isCurrentSession(session) {
    const sessionId = String(session?.session_id || "").trim();
    return Boolean(sessionId && currentSessionKey() === sessionId);
  }

  function reloadAfterCurrentSessionDelete() {
    try {
      window.location.reload();
    } catch (_error) {
      window.location.href = String(window.location.href || "");
    }
  }

  function stopRowActionEvent(event) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation?.();
  }

  function reactFiberFrom(element) {
    if (!element || typeof element !== "object") return null;
    const fiberKey = Object.keys(element).find((key) => key.startsWith("__reactFiber"));
    return fiberKey ? element[fiberKey] : null;
  }

  function authContextValueFrom(element) {
    for (let fiber = reactFiberFrom(element); fiber; fiber = fiber.return) {
      for (const value of [fiber.memoizedProps?.value, fiber.pendingProps?.value]) {
        if (value && typeof value === "object" && typeof value.setAuthMethod === "function" && "authMethod" in value) {
          return value;
        }
      }
    }
    return null;
  }

  function spoofChatGPTAuthMethod(element) {
    const auth = authContextValueFrom(element);
    if (!auth || auth.authMethod === "chatgpt") return false;
    auth.setAuthMethod("chatgpt");
    return true;
  }

  function pluginEntryButton() {
    const byIcon = document.querySelector(`${selectors.pluginNavButton} ${selectors.pluginSvgPath}`)?.closest("button");
    if (byIcon) return byIcon;
    return Array.from(document.querySelectorAll(selectors.pluginNavButton))
      .find((button) => /^(插件|Plugins)(\s+-\s+.*)?$/i.test((button.textContent || "").trim())) || null;
  }

  function labelUnlockedPluginEntry(button) {
    const labelTextNode = Array.from(button.querySelectorAll("span, div")).reverse()
      .flatMap((node) => Array.from(node.childNodes))
      .find((node) => node.nodeType === 3 && /^(插件|Plugins)( - 已解锁| - Unlocked)?$/i.test((node.nodeValue || "").trim()));
    if (!labelTextNode) return;
    const current = (labelTextNode.nodeValue || "").trim();
    labelTextNode.nodeValue = /^Plugins/i.test(current) ? "Plugins - Unlocked" : "插件 - 已解锁";
  }

  function clearPluginEntryUnlockLabel(button) {
    const labelTextNode = Array.from(button.querySelectorAll("span, div")).reverse()
      .flatMap((node) => Array.from(node.childNodes))
      .find((node) => node.nodeType === 3 && /^(插件 - 已解锁|Plugins - Unlocked)$/i.test((node.nodeValue || "").trim()));
    if (!labelTextNode) return;
    labelTextNode.nodeValue = /^Plugins/i.test((labelTextNode.nodeValue || "").trim()) ? "Plugins" : "插件";
  }

  function enablePluginEntry() {
    if (!enhancementSettings.pluginEntryUnlock) return;
    const pluginButton = pluginEntryButton();
    if (!pluginButton) return;
    spoofChatGPTAuthMethod(pluginButton);
    pluginButton.disabled = false;
    pluginButton.removeAttribute("disabled");
    pluginButton.style.display = "";
    pluginButton.querySelectorAll("*").forEach((node) => {
      node.style.display = "";
    });
    labelUnlockedPluginEntry(pluginButton);
    const reactPropsKey = Object.keys(pluginButton).find((key) => key.startsWith("__reactProps"));
    if (reactPropsKey) {
      pluginButton[reactPropsKey].disabled = false;
    }
    if (pluginButton.dataset.codexPilotPluginEnabled === "true") return;
    pluginButton.dataset.codexPilotPluginEnabled = "true";
    pluginButton.addEventListener("click", () => {
      spoofChatGPTAuthMethod(pluginButton);
    }, true);
  }

  function pluginInstallCandidates() {
    const nodes = Array.from(document.querySelectorAll(selectors.disabledInstallButton));
    return Array.from(new Set(nodes.map((node) => node.closest?.("button, [role='button']") || node)));
  }

  function installButtonLabel(element) {
    return (element.textContent || "").trim();
  }

  function isInstallButtonLabel(text) {
    return /^安装\s*/.test(text) || /^Install\s*/i.test(text) || text === "强制安装";
  }

  function patchReactDisabledProps(element) {
    Object.keys(element)
      .filter((key) => key.startsWith("__reactProps"))
      .forEach((key) => {
        const props = element[key];
        if (!props || typeof props !== "object") return;
        props.disabled = false;
        props["aria-disabled"] = false;
        props["data-disabled"] = undefined;
      });
  }

  function clearDisabledState(element) {
    if (!(element instanceof HTMLElement)) return;
    if ("disabled" in element) element.disabled = false;
    element.removeAttribute("disabled");
    element.removeAttribute("aria-disabled");
    element.removeAttribute("data-disabled");
    element.removeAttribute("inert");
    element.classList.remove("disabled", "opacity-50", "cursor-not-allowed", "pointer-events-none");
    element.classList.add("codex-pilot-force-install-unlocked");
    element.style.pointerEvents = "auto";
    element.style.opacity = "";
    element.style.cursor = "pointer";
    element.tabIndex = 0;
    patchReactDisabledProps(element);
  }

  function installButtonUnlockNodes(button) {
    const nodes = [button];
    button.querySelectorAll?.("button, [role='button'], [disabled], [aria-disabled], [data-disabled], .cursor-not-allowed, .pointer-events-none")
      .forEach((node) => nodes.push(node));
    let parent = button.parentElement;
    for (let depth = 0; parent && depth < 3; depth += 1, parent = parent.parentElement) {
      if (parent.matches?.("button, [role='button'], [disabled], [aria-disabled], [data-disabled], .cursor-not-allowed, .pointer-events-none")) {
        nodes.push(parent);
      }
    }
    return Array.from(new Set(nodes));
  }

  function installForcedInstallGuard(button) {
    if (button.dataset.codexPilotForceInstallUnlocked === "true") return;
    button.dataset.codexPilotForceInstallUnlocked = "true";
    const keepUnlocked = () => installButtonUnlockNodes(button).forEach(clearDisabledState);
    ["pointerdown", "mousedown", "mouseup", "click", "focus"].forEach((eventName) => {
      button.addEventListener(eventName, keepUnlocked, true);
    });
  }

  function unblockButtonElement(button) {
    installButtonUnlockNodes(button).forEach(clearDisabledState);
    installForcedInstallGuard(button);
  }

  function unblockPluginInstallButtons() {
    if (!enhancementSettings.forcePluginInstall) return;
    pluginInstallCandidates().forEach((button) => {
      const text = installButtonLabel(button);
      if (!isInstallButtonLabel(text)) return;
      unblockButtonElement(button);
    });
  }

  function clearPluginPatchArtifacts() {
    const pluginButton = pluginEntryButton();
    if (pluginButton) {
      delete pluginButton.dataset.codexPilotPluginEnabled;
      clearPluginEntryUnlockLabel(pluginButton);
    }
  }

  function installRowActionEvents(button, onActivate) {
    ["pointerdown", "mousedown", "mouseup", "touchstart"].forEach((eventName) => {
      button.addEventListener(eventName, stopRowActionEvent, true);
    });
    button.addEventListener("click", onActivate, true);
  }

  function setIconButtonContent(button, label, svgPath) {
    button.setAttribute("aria-label", label);
    button.title = label;
    button.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${svgPath}</svg>`;
  }

  function showToast(message, undoToken) {
    document.querySelectorAll(".codex-pilot-toast").forEach((node) => node.remove());
    const toast = document.createElement("div");
    toast.className = "codex-pilot-toast";
    toast.textContent = message;
    if (undoToken) {
      const undo = document.createElement("button");
      undo.type = "button";
      undo.textContent = "撤销";
      undo.addEventListener("click", async (event) => {
        stopRowActionEvent(event);
        try {
          const response = await window.__CODEX_PILOT__.undoDelete(undoToken);
          const result = response.result || response;
          toast.textContent = result.message || "已撤销删除，请刷新侧边栏";
        } catch (error) {
          toast.textContent = String(error);
        }
        setTimeout(() => toast.remove(), 5000);
      }, true);
      toast.appendChild(undo);
    }
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 9000);
  }

  function downloadMarkdown(result, fallbackSessionId) {
    if (!result?.markdown) return false;
    const blob = new Blob([result.markdown], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = result.filename || `${fallbackSessionId}.md`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
    return true;
  }

  function downloadHtml(result, fallbackSessionId) {
    if (!result?.html) return false;
    const blob = new Blob([result.html], { type: "text/html;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = result.filename || `${fallbackSessionId}.html`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
    return true;
  }

  async function exportSession(session, notify = showToast) {
    const response = await window.__CODEX_PILOT__.exportMarkdown(sessionPayload(session));
    const result = response.result || response;
    if (response.status !== "ok" || result.status === "failed" || result.status === "not_found") {
      notify(result.message || response.message || "导出失败", null);
      return false;
    }
    downloadMarkdown(result, session.session_id);
    notify(result.filename ? `已导出：${result.filename}` : "已导出 Markdown", null);
    return true;
  }

  async function exportSessionHtml(session, notify = showToast) {
    const response = await window.__CODEX_PILOT__.exportHtml(sessionPayload(session));
    const result = response.result || response;
    if (response.status !== "ok" || result.status === "failed" || result.status === "not_found") {
      notify(result.message || response.message || "导出失败", null);
      return false;
    }
    downloadHtml(result, session.session_id);
    notify(result.filename ? `已导出：${result.filename}` : "已导出 HTML", null);
    return true;
  }

  async function deleteSession(session, row, notify = showToast) {
    const title = session.title || session.session_id;
    reportRendererEvent("delete_attempt", {
      requestedSessionId: session.session_id,
      currentSessionKey: currentSessionKey(),
      detectedSessionId: detectCurrentSession()?.session_id || "",
      rowSessionId: row?.getAttribute?.("data-app-action-sidebar-thread-id") || "",
      title,
      href: String(window.location.href || "")
    });
    if (!window.confirm(`确认删除“${title}”？删除前会创建可撤销备份。`)) {
      return false;
    }
    const deletingCurrentSession = isCurrentSession(session);
    if (deletingCurrentSession && !window.confirm("你正在删除当前打开的会话。删除成功后会刷新页面，确认继续？")) {
      return false;
    }
    const response = await window.__CODEX_PILOT__.deleteSession(sessionPayload(session));
    const result = response.result || response;
    if (response.status !== "ok" || result.status === "failed" || result.status === "not_found") {
      notify(result.message || response.message || "删除失败", null);
      return false;
    }
    lastUndoToken = result.undo_token || null;
    if (row) {
      const container = removableRowContainer(row);
      container?.remove();
    } else {
      syncDeletedSessionRow(session);
    }
    notify(result.message || "已删除会话", lastUndoToken);
    if (deletingCurrentSession) {
      setTimeout(() => reloadAfterCurrentSessionDelete(), 120);
    }
    return true;
  }

  function attachRowActions(row) {
    if (!row) return;
    const session = sessionRefFromRow(row, "row");
    if (!session?.session_id) return;
    row.dataset.codexPilotRow = "true";
    row.querySelector(selectors.threadTitle)?.classList?.add(rowTitleMaskClass);
    if (row.querySelector(`.${actionGroupClass}`)) return;
    const group = document.createElement("div");
    group.className = actionGroupClass;

    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.className = actionButtonClass;
    deleteButton.dataset.danger = "true";
    setIconButtonContent(
      deleteButton,
      "删除会话",
      '<path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/>'
    );
    installRowActionEvents(deleteButton, async (event) => {
      stopRowActionEvent(event);
      await deleteSession(session, row);
    });

    group.append(deleteButton);
    row.appendChild(group);
  }

  function archivePageHintVisible() {
    if (window.location.href.includes("archive")) return true;
    if (document.querySelector("[data-codex-pilot-archive-row]")) return true;
    const archiveNav = document.querySelector(selectors.archiveNav);
    return Boolean(archiveNav && String(archiveNav.className || "").includes("bg-token-list-hover-background"));
  }

  function archiveRows() {
    if (!archivePageHintVisible()) return [];
    const unarchiveButtons = Array.from(document.querySelectorAll("button"))
      .filter((button) => normalizeText(button.textContent) === "取消归档");
    return unarchiveButtons
      .map((button) => button.closest("[role='listitem']") || button.closest("li") || button.parentElement)
      .filter(Boolean);
  }

  function archiveRefFromRow(row) {
    const sidebarRef = sessionRefFromRow(row, "archive-row");
    if (sidebarRef?.session_id) return sidebarRef;
    const title = normalizeText((row.querySelector(selectors.threadTitle) || row).textContent)
      .replace("取消归档", "")
      .replace("删除", "")
      .replace("导出", "")
      .replace(/\d{4}年\d{1,2}月\d{1,2}日.*$/, "")
      .replace(/\s+·\s+.*$/, "")
      .trim()
      .slice(0, 160);
    return { session_id: "", title: title || "未命名会话", source: "archive-title" };
  }

  async function resolveArchiveSession(row) {
    const ref = archiveRefFromRow(row);
    if (ref.session_id) return ref;
    const response = await window.__CODEX_PILOT__.findArchivedThread(ref.title);
    const result = response.result || response;
    return result?.id ? { session_id: result.id, title: result.title || ref.title, source: "archive-lookup" } : ref;
  }

  function attachArchiveActions(row) {
    if (!row || row.dataset.codexPilotArchiveRow === "true") return;
    const unarchiveButton = Array.from(row.querySelectorAll("button"))
      .find((button) => normalizeText(button.textContent) === "取消归档");
    if (!unarchiveButton) return;
    row.dataset.codexPilotArchiveRow = "true";

    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.className = archiveActionClass;
    deleteButton.dataset.danger = "true";
    setIconButtonContent(
      deleteButton,
      "删除会话",
      '<path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/>'
    );
    installRowActionEvents(deleteButton, async (event) => {
      stopRowActionEvent(event);
      const session = await resolveArchiveSession(row);
      if (!session.session_id) {
        showToast("删除失败：未找到归档会话 ID", null);
        return;
      }
      await deleteSession(session, row);
    });

    unarchiveButton.insertAdjacentElement("afterend", deleteButton);
  }

  function installArchiveDeleteAll(rows) {
    const existing = document.querySelector("[data-codex-pilot-archive-delete-all]");
    if (!rows.length) {
      existing?.remove();
      return;
    }
    if (existing) return;
    const button = document.createElement("button");
    button.type = "button";
    button.className = `${archiveActionClass} codex-pilot-archive-bar`;
    button.dataset.codexPilotArchiveDeleteAll = "true";
    button.dataset.danger = "true";
    button.textContent = `删除全部归档 (${rows.length})`;
    button.addEventListener("click", async (event) => {
      stopRowActionEvent(event);
      const currentRows = archiveRows();
      if (!currentRows.length) return;
      if (!window.confirm(`确认删除全部 ${currentRows.length} 个归档会话？删除前会创建可撤销备份。`)) return;
      const resolved = [];
      for (const row of currentRows) {
        const session = await resolveArchiveSession(row);
        if (session.session_id) {
          resolved.push({ row, session });
        }
      }
      const currentEntry = resolved.find(({ session }) => isCurrentSession(session));
      if (currentEntry && !window.confirm("删除列表包含当前打开的会话。删除成功后会刷新页面，确认继续？")) return;
      let deleted = 0;
      for (const { row, session } of resolved) {
        const response = await window.__CODEX_PILOT__.deleteSession(sessionPayload(session));
        const result = response.result || response;
        if (response.status === "ok" && result.status !== "failed" && result.status !== "not_found") {
          row.remove();
          deleted += 1;
        }
      }
      showToast(`已删除 ${deleted} 个归档会话`, null);
      if (currentEntry && deleted > 0) {
        setTimeout(() => reloadAfterCurrentSessionDelete(), 120);
      }
    }, true);
    const heading = Array.from(document.querySelectorAll("h1, h2, h3"))
      .find((element) => ["已归档对话", "Archived conversations"].includes(normalizeText(element.textContent)));
    (heading || document.body).appendChild(button);
  }

  function refreshSessionActions() {
    if (!enhancementSettings.enabled || !enhancementSettings.inlineActions) return;
    sessionRows().forEach(attachRowActions);
    const rows = archiveRows();
    rows.forEach(attachArchiveActions);
    installArchiveDeleteAll(rows);
  }

  function currentSessionKey() {
    return (sessionRefFromUrl()?.session_id || detectCurrentSession()?.session_id || "").trim();
  }

  function readScrollStore() {
    try {
      const parsed = JSON.parse(window.localStorage?.getItem(scrollStoreKey) || "{}");
      return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
    } catch (_error) {
      return {};
    }
  }

  function writeScrollStore(store) {
    try {
      const entries = Object.entries(store)
        .filter(([key, value]) => key && value && typeof value === "object")
        .sort((left, right) => Number(right[1].at || 0) - Number(left[1].at || 0))
        .slice(0, maxScrollEntries);
      window.localStorage?.setItem(scrollStoreKey, JSON.stringify(Object.fromEntries(entries)));
      return true;
    } catch (error) {
      reportRendererEvent("scroll_restore_error", { message: String(error), phase: "write_store" });
      return false;
    }
  }

  function finiteScrollNumber(value) {
    const number = Number(value);
    return Number.isFinite(number) && number >= 0 ? number : 0;
  }

  function scrollContainerCandidates() {
    return [
      document.querySelector(".thread-scroll-container"),
      document.querySelector("[data-testid='conversation-turn-list']")?.parentElement,
      document.querySelector("main")?.parentElement,
      document.scrollingElement,
      document.documentElement,
      document.body
    ].filter(Boolean);
  }

  function scrollMetric(element, key) {
    if (element === document.body || element === document.documentElement || element === document.scrollingElement) {
      if (key === "scrollTop") return finiteScrollNumber(window.scrollY || document.documentElement.scrollTop || document.body.scrollTop);
      if (key === "clientHeight") return finiteScrollNumber(window.innerHeight || document.documentElement.clientHeight);
      if (key === "scrollHeight") return finiteScrollNumber(document.documentElement.scrollHeight || document.body.scrollHeight);
    }
    return finiteScrollNumber(element?.[key]);
  }

  function currentThreadScroller() {
    const candidates = scrollContainerCandidates();
    return candidates.find((element) => {
      const scrollHeight = scrollMetric(element, "scrollHeight");
      const clientHeight = scrollMetric(element, "clientHeight");
      return scrollHeight > clientHeight + 80;
    }) || document.scrollingElement || document.documentElement || document.body;
  }

  function setScrollTop(element, top) {
    const target = Math.max(0, finiteScrollNumber(top));
    if (element === document.body || element === document.documentElement || element === document.scrollingElement) {
      window.scrollTo?.({ top: target, behavior: "auto" });
      document.documentElement.scrollTop = target;
      document.body.scrollTop = target;
      return;
    }
    element.scrollTop = target;
  }

  function saveThreadScrollPosition(reason = "periodic") {
    const session = detectCurrentSession();
    const sessionId = session?.session_id;
    if (!sessionId) return false;
    const scroller = currentThreadScroller();
    const top = scrollMetric(scroller, "scrollTop");
    if (!scroller || top < 0) return false;
    const store = readScrollStore();
    store[sessionId] = {
      top,
      at: Date.now(),
      title: session.title || "",
      href: String(window.location.href || "")
    };
    if (writeScrollStore(store)) {
      reportRendererEvent("scroll_restore_saved", { session_id: sessionId, top, reason });
      return true;
    }
    return false;
  }

  function scheduleSaveThreadScroll(reason = "scroll") {
    if (Date.now() < restoreInProgressUntil) return;
    if (scrollSaveTimer) return;
    scrollSaveTimer = setTimeout(() => {
      scrollSaveTimer = null;
      saveThreadScrollPosition(reason);
    }, 160);
  }

  function userScrollIntentActive() {
    return Date.now() < userScrollIntentUntil;
  }

  function restoreThreadScrollPosition(sessionId, attempt = 0) {
    if (!sessionId) return;
    const store = readScrollStore();
    const entry = store[sessionId];
    if (!entry || !Number.isFinite(Number(entry.top))) {
      reportRendererEvent("scroll_restore_skipped", { session_id: sessionId, reason: "no_entry" });
      return;
    }
    if (userScrollIntentActive()) {
      reportRendererEvent("scroll_restore_skipped", { session_id: sessionId, reason: "user_scroll" });
      return;
    }
    const scroller = currentThreadScroller();
    const scrollHeight = scrollMetric(scroller, "scrollHeight");
    const clientHeight = scrollMetric(scroller, "clientHeight");
    if (scrollHeight <= clientHeight + 80 && attempt < 5) {
      setTimeout(() => restoreThreadScrollPosition(sessionId, attempt + 1), 180 + attempt * 160);
      return;
    }
    const maxTop = Math.max(0, scrollHeight - clientHeight);
    const targetTop = Math.min(finiteScrollNumber(entry.top), maxTop);
    if (maxTop <= 0) {
      reportRendererEvent("scroll_restore_skipped", { session_id: sessionId, reason: "no_scroll_range" });
      return;
    }
    restoreInProgressUntil = Date.now() + 700;
    setScrollTop(scroller, targetTop);
    reportRendererEvent("scroll_restore_applied", {
      session_id: sessionId,
      top: targetTop,
      attempt
    });
  }

  function handleThreadMaybeChanged(reason = "poll") {
    const nextSessionId = currentSessionKey();
    if (!nextSessionId || nextSessionId === activeScrollSessionId) {
      refreshFastToggleUi();
      return;
    }
    if (activeScrollSessionId) {
      saveThreadScrollPosition(`before_${reason}`);
    }
    const previousSessionId = activeScrollSessionId;
    activeScrollSessionId = nextSessionId;
    reportRendererEvent("thread_changed", {
      previous_session_id: previousSessionId,
      session_id: nextSessionId,
      reason
    });
    [80, 260, 620, 1200].forEach((delay, index) => {
      setTimeout(() => restoreThreadScrollPosition(nextSessionId, index), delay);
    });
    refreshTimelineSoon();
    refreshFastToggleUi();
  }

  function maybeBindDraftToCurrentSession(reason = "unknown", sessionId = currentServiceTierBindSessionKey()) {
    const normalizedSessionId = validServiceTierSessionKey(sessionId);
    if (!normalizedSessionId) return false;
    const draft = currentServiceTierDraft();
    if (!draft || !draft.pendingBind) return false;
    if (Date.now() - draft.at > serviceTierDraftTtlMs) {
      clearServiceTierDraft("bind_timeout");
      reportRendererEvent("thread_fast_bind_expired", { reason, session_id: normalizedSessionId });
      return false;
    }
    if (draft.previousSessionId && draft.previousSessionId === normalizedSessionId) {
      reportRendererEvent("thread_fast_bind_deferred", {
        reason,
        session_id: normalizedSessionId,
        cause: "previous_session"
      });
      return false;
    }
    if (draft.existingSessionIds?.includes(normalizedSessionId)) {
      reportRendererEvent("thread_fast_bind_deferred", {
        reason,
        session_id: normalizedSessionId,
        cause: "existing_sidebar_session"
      });
      return false;
    }
    const existing = currentServiceTierThreadOverride(normalizedSessionId);
    if (existing && existing.at >= draft.at) {
      clearServiceTierDraft("existing_thread_override_newer");
      reportRendererEvent("thread_fast_bind_skipped", {
        reason,
        session_id: normalizedSessionId,
        existing_mode: existing.mode
      });
      return false;
    }
    const wrote = setServiceTierThreadOverride(normalizedSessionId, draft.mode);
    if (!wrote) return false;
    clearServiceTierDraft("bound");
    reportRendererEvent("thread_fast_bound", {
      reason,
      session_id: normalizedSessionId,
      mode: draft.mode
    });
    return true;
  }

  function supportedServiceTierMethods() {
    return new Set(["thread/start", "thread/resume", "turn/start"]);
  }

  function serviceTierRequestOverrideForMethod(method, params, threadIdHint = "") {
    if (!supportedServiceTierMethods().has(method) || !params || typeof params !== "object") return null;
    const threadId = method === "thread/start"
      ? validServiceTierSessionKey(params.threadId || threadIdHint)
      : validServiceTierSessionKey(params.threadId || params.conversationId || threadIdHint || currentServiceTierSessionKey());
    if (enhancementSettings.fastGlobalMode) {
      return {
        source: "global",
        sessionId: threadId,
        mode: "fast",
        serviceTier: fastServiceTierValue
      };
    }
    const threadOverride = currentServiceTierThreadOverride(threadId);
    if (threadOverride) {
      return {
        source: "thread",
        sessionId: threadId,
        mode: threadOverride.mode,
        serviceTier: isFastMode(threadOverride.mode) ? fastServiceTierValue : null
      };
    }
    if (method === "thread/start") {
      const draft = currentServiceTierDraftForThreadStart(threadId);
      if (draft) {
        return {
          source: "draft",
          sessionId: "",
          mode: draft.mode,
          serviceTier: isFastMode(draft.mode) ? fastServiceTierValue : null
        };
      }
    }
    return null;
  }

  function applyServiceTierRequestOverride(method, params, threadIdHint = "") {
    const override = serviceTierRequestOverrideForMethod(method, params, threadIdHint);
    if (!override) return { changed: false, params };
    const nextParams = withServiceTierOverride(params, override.serviceTier);
    if (override.source === "draft") {
      const startToken = `${Date.now()}-${Math.random()}`;
      fastPendingBindStartToken = startToken;
      markServiceTierDraftPendingBind(startToken);
      fastPendingBindStartedAt = Date.now();
    }
    reportRendererEvent("thread_fast_request_override_applied", {
      method,
      source: override.source,
      session_id: override.sessionId,
      mode: override.mode,
      service_tier: override.serviceTier || "standard"
    });
    return { changed: true, params: nextParams };
  }

  function withServiceTierOverride(params, serviceTier) {
    const nextParams = { ...(params || {}), serviceTier };
    if (Object.prototype.hasOwnProperty.call(nextParams, "service_tier")) {
      nextParams.service_tier = serviceTier;
    }
    return nextParams;
  }

  function serviceTierOverrideResultForMessage(message) {
    if (!message || typeof message !== "object") return { changed: false, message };
    if (message.type === "send-cli-request-for-host") {
      const method = String(message.method || "");
      if (!method || !message.params || typeof message.params !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_method_or_params" });
        return { changed: false, message };
      }
      const result = applyServiceTierRequestOverride(method, message.params);
      return result.changed
        ? { changed: true, message: { ...message, params: result.params } }
        : { changed: false, message };
    }
    if (message.type === "mcp-request") {
      if (!message.request || typeof message.request !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_request" });
        return { changed: false, message };
      }
      const method = String(message.request.method || "");
      if (!method || !message.request.params || typeof message.request.params !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_method_or_params" });
        return { changed: false, message };
      }
      const result = applyServiceTierRequestOverride(method, message.request.params);
      return result.changed
        ? { changed: true, message: { ...message, request: { ...message.request, params: result.params } } }
        : { changed: false, message };
    }
    if (message.type === "worker-request") {
      if (!message.request || typeof message.request !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_request" });
        return { changed: false, message };
      }
      const method = String(message.request.method || "");
      if (!method || !message.request.params || typeof message.request.params !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_method_or_params" });
        return { changed: false, message };
      }
      const result = applyServiceTierRequestOverride(method, message.request.params);
      return result.changed
        ? { changed: true, message: { ...message, request: { ...message.request, params: result.params } } }
        : { changed: false, message };
    }
    if (message.type === "thread-prewarm-start") {
      if (!message.request || typeof message.request !== "object" || !message.request.params || typeof message.request.params !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_request_params" });
        return { changed: false, message };
      }
      const result = applyServiceTierRequestOverride("thread/start", message.request.params);
      return result.changed
        ? { changed: true, message: { ...message, request: { ...message.request, params: result.params } } }
        : { changed: false, message };
    }
    if (message.type === "start-conversation") {
      if (enhancementSettings.fastGlobalMode) {
        reportRendererEvent("thread_fast_request_override_applied", {
          method: "start-conversation",
          source: "global",
          session_id: "",
          mode: "fast",
          service_tier: fastServiceTierValue
        });
        return { changed: true, message: withServiceTierOverride(message, fastServiceTierValue) };
      }
      const draft = currentServiceTierDraft();
      if (!draft) return { changed: false, message };
      const startToken = `${Date.now()}-${Math.random()}`;
      fastPendingBindStartToken = startToken;
      markServiceTierDraftPendingBind(startToken);
      fastPendingBindStartedAt = Date.now();
      reportRendererEvent("thread_fast_request_override_applied", {
        method: "start-conversation",
        source: "draft",
        session_id: "",
        mode: draft.mode,
        service_tier: isFastMode(draft.mode) ? fastServiceTierValue : "standard"
      });
      return { changed: true, message: withServiceTierOverride(message, isFastMode(draft.mode) ? fastServiceTierValue : null) };
    }
    if (message.type === "prewarm-thread-start-for-host") {
      if (!message.params || typeof message.params !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_params" });
        return { changed: false, message };
      }
      const result = applyServiceTierRequestOverride("thread/start", message.params);
      return result.changed
        ? { changed: true, message: { ...message, params: result.params } }
        : { changed: false, message };
    }
    if (message.type === "start-thread-for-host") {
      const result = applyServiceTierRequestOverride("thread/start", message);
      return result.changed
        ? { changed: true, message: result.params }
        : { changed: false, message };
    }
    if (message.type === "start-turn-for-host") {
      if (!message.params || typeof message.params !== "object") {
        reportRendererEvent("thread_fast_request_override_unsupported", { type: message.type, reason: "missing_params" });
        return { changed: false, message };
      }
      const result = applyServiceTierRequestOverride("turn/start", message.params, message.conversationId);
      return result.changed
        ? { changed: true, message: { ...message, params: result.params } }
        : { changed: false, message };
    }
    return { changed: false, message };
  }

  function overrideServiceTierForMessage(message) {
    return serviceTierOverrideResultForMessage(message).message;
  }

  function installServiceTierTestApi() {
    if (!window.__CODEX_PILOT_TEST__) return;
    window.__CODEX_PILOT_FAST_TEST__ = {
      clear() {
        window.localStorage?.removeItem?.(serviceTierStoreKey);
        fastPendingBindStartedAt = 0;
        fastPendingBindStartToken = "";
        refreshFastToggleUi();
      },
      state() {
        return readServiceTierStore();
      },
      uiState() {
        return currentServiceTierUiState();
      },
      setThread(sessionId, mode) {
        return setServiceTierThreadOverride(sessionId, mode);
      },
      setDraft(mode) {
        return setServiceTierDraft(mode, false);
      },
      override(message) {
        return overrideServiceTierForMessage(message);
      },
      bind(reason = "test", sessionId = currentServiceTierBindSessionKey()) {
        return maybeBindDraftToCurrentSession(reason, sessionId);
      }
    };
  }

  function resolveServiceTierDispatcher(module) {
    const exports = module && typeof module === "object" ? module : {};
    const candidates = Object.entries(exports);
    if (Object.prototype.hasOwnProperty.call(exports, "default")) {
      candidates.unshift(["default", exports.default]);
    }
    // 第一轮：只认「直接带 dispatchMessage 的导出」——即 vscode-api 的单例实例（Er=f）。
    // 命中即返回，避免去对其它导出调用 getInstance 而产生副作用。
    for (const [exportName, candidate] of candidates) {
      if (
        candidate &&
        (typeof candidate === "object" || typeof candidate === "function") &&
        typeof candidate.dispatchMessage === "function"
      ) {
        return { dispatcher: candidate, exportName };
      }
    }
    // 第二轮：退回 getInstance 工厂解析（旧版构建里 dispatcher 以类形式导出）。
    for (const [exportName, candidate] of candidates) {
      const dispatcher = dispatcherFromCandidate(candidate);
      if (dispatcher) return { dispatcher, exportName };
    }
    return null;
  }

  function dispatcherFromCandidate(candidate) {
    if (!candidate || (typeof candidate !== "object" && typeof candidate !== "function")) return null;
    if (typeof candidate.dispatchMessage === "function") return candidate;
    const getInstance = candidate.getInstance;
    if (typeof getInstance !== "function") return null;
    if (!candidateLooksLikeDispatcherFactory(candidate)) return null;
    try {
      const instance = getInstance.call(candidate);
      return instance && typeof instance.dispatchMessage === "function" ? instance : null;
    } catch (_error) {
      return null;
    }
  }

  function candidateLooksLikeDispatcherFactory(candidate) {
    if (!candidate || (typeof candidate !== "object" && typeof candidate !== "function")) return false;
    if (typeof candidate.prototype?.dispatchMessage === "function") return true;
    try {
      return Function.prototype.toString.call(candidate).includes("dispatchMessage");
    } catch (_error) {
      return false;
    }
  }

  function installServiceTierDispatcherPatch() {
    if (fastDispatcherPatchInstalled) return;
    if (fastDispatcherPatchPromise) return;
    fastDispatcherPatchStatus = "loading";
    refreshFastToggleUi();
    fastDispatcherPatchPromise = Promise.resolve().then(async () => {
      try {
        const module = await loadCodexAppModule("vscode-api-");
        if (!isActiveBoot() || !enhancementSettings.enabled) {
          fastDispatcherPatchInstalled = false;
          fastDispatcherPatchStatus = "unavailable";
          fastDispatcherPatchPromise = null;
          refreshFastToggleUi();
          return;
        }
        const resolved = resolveServiceTierDispatcher(module);
        const dispatcher = resolved?.dispatcher;
        if (!dispatcher) {
          const exportNames = Object.keys(module || {}).slice(0, 16).join(",");
          throw new Error(`Codex dispatcher unavailable: no dispatcher export${exportNames ? ` (${exportNames})` : ""}`);
        }
        const existingPatchActive = hasActiveServiceTierDispatcherPatch(dispatcher);
        const originalDispatch = existingPatchActive && typeof dispatcher.__codexPilotOriginalDispatchMessage === "function"
          ? dispatcher.__codexPilotOriginalDispatchMessage
          : dispatcher.dispatchMessage.bind(dispatcher);
        if (!existingPatchActive) {
          clearServiceTierDispatcherPatchMetadata(dispatcher);
        }
        if (
          dispatcher.__codexPilotFastPatchVersion === fastDispatcherPatchVersion &&
          dispatcher.__codexPilotFastPatchOwnerBootId === bootId &&
          existingPatchActive
        ) {
          rememberServiceTierDispatcherPatch(
            dispatcher,
            originalDispatch,
            dispatcher.__codexPilotFastPatchedDispatchMessage || dispatcher.dispatchMessage
          );
          fastDispatcherPatchInstalled = true;
          fastDispatcherPatchStatus = "ready";
          window.__codexPilotFastDispatcherPatchInstalled = fastDispatcherPatchVersion;
          refreshFastToggleUi();
          return;
        }
        dispatcher.__codexPilotOriginalDispatchMessage = originalDispatch;
        const patchedDispatch = (type, payload) => {
          if (!isActiveBoot() || !enhancementSettings.enabled) {
            if (dispatcher.dispatchMessage === patchedDispatch) {
              restoreServiceTierDispatcherPatch(dispatcher);
            }
            fastDispatcherPatchInstalled = false;
            fastDispatcherPatchStatus = "unavailable";
            return originalDispatch(type, payload);
          }
          if (!payload || typeof payload !== "object") {
            return originalDispatch(type, payload);
          }
          const message = { ...payload, type };
          const result = serviceTierOverrideResultForMessage(message);
          if (!result.changed) {
            return originalDispatch(type, payload);
          }
          const nextType = result.message?.type || type;
          const { type: _type, ...nextPayload } = result.message || {};
          return originalDispatch(nextType, nextPayload);
        };
        dispatcher.dispatchMessage = patchedDispatch;
        dispatcher.__codexPilotFastPatchVersion = fastDispatcherPatchVersion;
        dispatcher.__codexPilotFastPatchOwnerBootId = bootId;
        dispatcher.__codexPilotFastPatchedDispatchMessage = patchedDispatch;
        rememberServiceTierDispatcherPatch(dispatcher, originalDispatch, patchedDispatch);
        fastDispatcherPatchInstalled = true;
        fastDispatcherPatchStatus = "ready";
        window.__codexPilotFastDispatcherPatchInstalled = fastDispatcherPatchVersion;
        refreshFastToggleUi();
        reportRendererEvent("thread_fast_dispatcher_patch_installed", { export_name: resolved.exportName || "" });
      } catch (error) {
        fastDispatcherPatchStatus = "unavailable";
        reportRendererEvent("thread_fast_dispatcher_patch_failed", { message: String(error) });
        fastDispatcherPatchPromise = null;
        refreshFastToggleUi();
      }
    });
  }

  function hasActiveServiceTierDispatcherPatch(dispatcher) {
    if (!dispatcher?.__codexPilotFastPatchVersion || typeof dispatcher.dispatchMessage !== "function") return false;
    const patchedDispatch = dispatcher.__codexPilotFastPatchedDispatchMessage;
    if (typeof patchedDispatch === "function") {
      return dispatcher.dispatchMessage === patchedDispatch;
    }
    if (typeof dispatcher.__codexPilotOriginalDispatchMessage !== "function") return false;
    const source = Function.prototype.toString.call(dispatcher.dispatchMessage);
    return source.includes("overrideServiceTierForMessage")
      || source.includes("serviceTierOverrideResultForMessage")
      || source.includes("thread_fast_request_override");
  }

  function clearServiceTierDispatcherPatchMetadata(dispatcher) {
    if (!dispatcher) return;
    delete dispatcher.__codexPilotOriginalDispatchMessage;
    delete dispatcher.__codexPilotFastPatchVersion;
    delete dispatcher.__codexPilotFastPatchOwnerBootId;
    delete dispatcher.__codexPilotFastPatchedDispatchMessage;
    if (window.__codexPilotFastDispatcherPatch?.dispatcher === dispatcher) {
      delete window.__codexPilotFastDispatcherPatch;
    }
    delete window.__codexPilotFastDispatcherPatchInstalled;
  }

  function rememberServiceTierDispatcherPatch(dispatcher, originalDispatch, patchedDispatch) {
    if (!dispatcher || typeof originalDispatch !== "function") return;
    window.__codexPilotFastDispatcherPatch = {
      version: fastDispatcherPatchVersion,
      ownerBootId: bootId,
      dispatcher,
      originalDispatch,
      patchedDispatch
    };
  }

  function restoreServiceTierDispatcherPatch(dispatcher, options = {}) {
    const registry = window.__codexPilotFastDispatcherPatch;
    const registered = Boolean(dispatcher && registry?.dispatcher === dispatcher);
    const ownerBootId = String(dispatcher?.__codexPilotFastPatchOwnerBootId || (registered ? registry?.ownerBootId : "") || "");
    const expectedOwnerBootId = String(options.ownerBootId || "");
    const allowAnyOwner = options.allowAnyOwner === true;
    if (expectedOwnerBootId && ownerBootId && ownerBootId !== expectedOwnerBootId) {
      return false;
    }
    if (!expectedOwnerBootId && !allowAnyOwner && ownerBootId && ownerBootId !== bootId) {
      return false;
    }
    const originalDispatch = dispatcher?.__codexPilotOriginalDispatchMessage
      || (registered ? registry?.originalDispatch : null);
    const activePatch = hasActiveServiceTierDispatcherPatch(dispatcher)
      || (registered && dispatcher?.dispatchMessage === registry?.patchedDispatch);
    if (
      !dispatcher ||
      typeof originalDispatch !== "function" ||
      !activePatch
    ) {
      if (registered && (allowAnyOwner || !ownerBootId || ownerBootId === bootId || ownerBootId === expectedOwnerBootId)) {
        delete window.__codexPilotFastDispatcherPatch;
      }
      return false;
    }
    dispatcher.dispatchMessage = originalDispatch;
    clearServiceTierDispatcherPatchMetadata(dispatcher);
    delete window.__codexPilotFastDispatcherPatchInstalled;
    if (registered) {
      delete window.__codexPilotFastDispatcherPatch;
    }
    return true;
  }

  async function uninstallServiceTierDispatcherPatch(reason = "disabled") {
    try {
      const registry = window.__codexPilotFastDispatcherPatch;
      const canRestoreRegisteredPatch = isActiveBoot() && !enhancementSettings.enabled;
      if (!restoreServiceTierDispatcherPatch(registry?.dispatcher, { allowAnyOwner: canRestoreRegisteredPatch })) {
        const module = await loadCodexAppModule("vscode-api-");
        if (!isActiveBoot() || enhancementSettings.enabled) {
          return;
        }
        const dispatcher = resolveServiceTierDispatcher(module)?.dispatcher;
        restoreServiceTierDispatcherPatch(dispatcher, { allowAnyOwner: true });
      }
    } catch (error) {
      reportRendererEvent("thread_fast_dispatcher_patch_uninstall_failed", { reason, message: String(error) });
    } finally {
      fastDispatcherPatchInstalled = false;
      fastDispatcherPatchStatus = "unavailable";
      fastDispatcherPatchPromise = null;
      refreshFastToggleUi();
    }
  }

  function installScrollRestore() {
    if (!enhancementSettings.enabled || !enhancementSettings.scrollRestore) return;
    activeScrollSessionId = currentSessionKey();
    const markUserScrollIntent = () => {
      if (!isActiveBoot()) return;
      if (Date.now() >= restoreInProgressUntil) {
        userScrollIntentUntil = Date.now() + 1000;
      }
      scheduleSaveThreadScroll("scroll");
    };
    document.addEventListener("scroll", markUserScrollIntent, true);
    document.addEventListener("wheel", () => {
      if (!isActiveBoot()) return;
      userScrollIntentUntil = Date.now() + 1200;
    }, true);
    document.addEventListener("touchmove", () => {
      if (!isActiveBoot()) return;
      userScrollIntentUntil = Date.now() + 1200;
    }, true);
    document.addEventListener("pointerdown", (event) => {
      if (!isActiveBoot()) return;
      const row = event.target?.closest?.(selectors.sidebarThread);
      if (!row) return;
      saveThreadScrollPosition("before_sidebar_navigation");
      const ref = sessionRefFromRow(row, "navigation");
      reportRendererEvent("thread_changed", {
        previous_session_id: activeScrollSessionId,
        session_id: ref?.session_id || "",
        reason: "sidebar_pointer"
      });
    }, true);
    window.addEventListener?.("beforeunload", () => saveThreadScrollPosition("beforeunload"));
    reportRendererEvent("scroll_restore_ready", { session_id: activeScrollSessionId });
  }

  function timelineTextFromNode(node) {
    return normalizeText(node?.textContent || "")
      .replace(/\b(CodexPilot|导出 Markdown|删除会话)\b/g, "")
      .trim()
      .slice(0, 120);
  }

  function timelineRoleSignal(node) {
    const samples = [];
    let current = node;
    let depth = 0;
    while (current && depth < 4) {
      const attrs = [
        current.getAttribute?.("data-message-author-role"),
        current.getAttribute?.("data-author-role"),
        current.getAttribute?.("data-role"),
        current.getAttribute?.("data-testid"),
        current.getAttribute?.("aria-label"),
        current.getAttribute?.("aria-roledescription"),
        typeof current.className === "string" ? current.className : String(current.className || "")
      ]
        .filter(Boolean)
        .join(" ");
      if (attrs) {
        samples.push(attrs.toLowerCase());
      }
      current = current.parentElement;
      depth += 1;
    }
    const signal = samples.join(" ");
    if (!signal) return "unknown";
    if (/(assistant|codex|tool|system|model)-?(message|turn)?/.test(signal)) return "assistant";
    if (/(user|human|prompt|request)-?(message|turn)?/.test(signal)) return "user";
    return "unknown";
  }

  function isReasonableTimelineNode(node, text) {
    if (!node || !text || text.length < 2) return false;
    const rect = node.getBoundingClientRect?.();
    if (rect && (rect.width <= 0 || rect.height <= 0)) return false;
    if (text.length > 4000) return false;
    const nestedConversationBlocks = node.querySelectorAll?.(
      "[data-message-author-role], [data-author-role], [data-testid*='conversation'], [data-testid*='message'], article"
    )?.length || 0;
    if (nestedConversationBlocks > 6) return false;
    return true;
  }

  function isLikelyUserMessageNode(node, text) {
    if (!node) return false;
    const roleSignal = timelineRoleSignal(node);
    if (roleSignal === "assistant") return false;
    if (roleSignal === "user") return isReasonableTimelineNode(node, text);
    const normalizedText = text || normalizeText(node.textContent || "");
    if (/^(you|user|你|用户)[:：\s]/i.test(normalizedText)) {
      return isReasonableTimelineNode(node, normalizedText);
    }
    if (/^(assistant|codex|助手)[:：\s]/i.test(normalizedText)) {
      return false;
    }
    const testId = String(node.getAttribute?.("data-testid") || "");
    if (/conversation-turn|message/i.test(testId)) {
      return isReasonableTimelineNode(node, normalizedText);
    }
    return false;
  }

  function pushTimelineCandidate(nodes, candidate) {
    const candidateNode = candidate?.node;
    if (!candidateNode) return;
    const existingIndex = nodes.findIndex((item) => item.node === candidateNode || item.node.contains(candidateNode));
    if (existingIndex >= 0) {
      nodes[existingIndex] = candidate;
      return;
    }
    if (nodes.some((item) => candidateNode.contains(item.node))) {
      return;
    }
    nodes.push(candidate);
  }

  function timelineNodeOffset(node, scroller, fallbackOffset) {
    const rect = node.getBoundingClientRect?.();
    const top = Number(rect?.top);
    if (Number.isFinite(top) && top !== 0) {
      return top + scrollMetric(scroller, "scrollTop");
    }
    if (typeof node.offsetTop === "number" && node.offsetTop > 0) {
      return node.offsetTop;
    }
    return fallbackOffset;
  }

  function messageCandidates() {
    const selectors = [
      "[data-message-author-role='user']",
      "[data-author-role='user']",
      "[data-testid*='conversation-turn']",
      "[data-testid*='conversation']",
      "[data-testid*='user-message']",
      "[data-testid*='message']",
      "[class*='user-message']",
      "[class*='prompt']",
      "[class*='conversation']",
      "[data-message-id]",
      "article",
      "[role='article']"
    ];
    const seen = new Set();
    const nodes = [];
    selectors.forEach((selector) => {
      document.querySelectorAll(selector).forEach((node) => {
        if (seen.has(node)) return;
        seen.add(node);
        const text = timelineTextFromNode(node);
        if (!isLikelyUserMessageNode(node, text)) return;
        pushTimelineCandidate(nodes, { node, text });
      });
    });
    return nodes.slice(0, 80);
  }

  function removeTimeline() {
    document.getElementById(timelineRootId)?.remove();
    lastTimelineSignature = "";
  }

  function renderTimeline() {
    if (!enhancementSettings.enabled || !enhancementSettings.timeline) {
      removeTimeline();
      return;
    }
    try {
      const sessionId = currentSessionKey();
      if (!sessionId) {
        removeTimeline();
        return;
      }
      const items = messageCandidates();
      if (items.length < 2) {
        removeTimeline();
        if (Date.now() - lastTimelineNoTargetsAt > 5000) {
          lastTimelineNoTargetsAt = Date.now();
          reportRendererEvent("timeline_no_targets", { session_id: sessionId, count: items.length });
        }
        return;
      }
      const signature = `${sessionId}:${items.length}:${items.map((item) => item.text.slice(0, 12)).join("|")}`;
      if (signature === lastTimelineSignature && document.getElementById(timelineRootId)) return;
      lastTimelineSignature = signature;
      document.getElementById(timelineRootId)?.remove();

      const scroller = currentThreadScroller();
      const scrollHeight = Math.max(scrollMetric(scroller, "scrollHeight"), 1);
      const root = document.createElement("div");
      root.id = timelineRootId;
      root.className = "codex-pilot-timeline";
      const track = document.createElement("div");
      track.className = "codex-pilot-timeline-track";
      root.appendChild(track);
      items.forEach((item, index) => {
        const fallbackOffset = index * (scrollHeight / Math.max(items.length - 1, 1));
        const viewportOffset = timelineNodeOffset(item.node, scroller, fallbackOffset);
        const percent = Math.max(2, Math.min(98, (viewportOffset / scrollHeight) * 100));
        const marker = document.createElement("button");
        marker.type = "button";
        marker.className = "codex-pilot-timeline-marker";
        marker.style.top = `${percent}%`;
        marker.setAttribute("aria-label", `跳转到第 ${index + 1} 个问题`);
        const tooltip = document.createElement("span");
        tooltip.className = "codex-pilot-timeline-tooltip";
        tooltip.textContent = item.text;
        marker.appendChild(tooltip);
        marker.addEventListener("click", (event) => {
          stopRowActionEvent(event);
          reportRendererEvent("timeline_jump", {
            session_id: sessionId,
            index,
            text: item.text.slice(0, 80)
          });
          item.node.scrollIntoView?.({ block: "center", behavior: "smooth" });
        }, true);
        root.appendChild(marker);
      });
      document.body.appendChild(root);
      reportRendererEvent("timeline_rendered", { session_id: sessionId, count: items.length });
    } catch (error) {
      removeTimeline();
      reportRendererEvent("timeline_error", { message: String(error) });
    }
  }

  function refreshTimelineSoon() {
    setTimeout(renderTimeline, 180);
  }

  let backendStatusCheckSeq = 0;

  function formatBackendStatusMessage(result) {
    return result.status === "ok"
      ? `${result.message || "后端已连接"} (${result.transport || "bridge"})`
      : result.message || "后端检查失败";
  }

  function backendStatusWithTimeout() {
    const request = window.__CODEX_PILOT__.backendStatus();
    if (typeof window.setTimeout !== "function") return request;
    let timer = null;
    return Promise.race([
      request.then((result) => {
        if (timer && typeof window.clearTimeout === "function") {
          window.clearTimeout(timer);
        }
        return result;
      }),
      new Promise((resolve) => {
        timer = window.setTimeout(() => {
          reportRendererEvent("backend_status_timeout", {
            helper_port: window.__CODEX_PILOT__?.helperPort,
            has_bridge: typeof window.__codexPilotBridge === "function",
            href: String(window.location.href || ""),
            title: document.title || ""
          });
          resolve({
            status: "timeout",
            message: "后端检查超时"
          });
        }, backendStatusTimeoutMs);
      })
    ]);
  }

  async function maybeRecoverBackendBridge(result) {
    if (result?.status !== "timeout") {
      consecutiveBackendTimeouts = 0;
      return;
    }
    consecutiveBackendTimeouts += 1;
    if (consecutiveBackendTimeouts < backendRecoveryThreshold || recoveryInFlight) {
      return;
    }
    const now = Date.now();
    if (now - lastRecoveryAttemptAt < backendRecoveryCooldownMs) {
      return;
    }
    lastRecoveryAttemptAt = now;
    recoveryInFlight = true;
    reportRendererEvent("backend_recovery_requested", {
      consecutive_timeouts: consecutiveBackendTimeouts,
      helper_port: window.__CODEX_PILOT__?.helperPort,
      has_bridge: typeof window.__codexPilotBridge === "function"
    });
    try {
      const recovery = await window.__CODEX_PILOT__.recoverBridge();
      reportRendererEvent("backend_recovery_result", {
        status: recovery?.status || "unknown",
        message: recovery?.message || ""
      });
      if (recovery?.status === "ok") {
        consecutiveBackendTimeouts = 0;
      }
    } catch (error) {
      reportRendererEvent("backend_recovery_error", { message: String(error) });
    } finally {
      recoveryInFlight = false;
    }
  }

  async function refreshBackendStatus(root, message) {
    const seq = ++backendStatusCheckSeq;
    root.dataset.status = "checking";
    message.textContent = "正在检查后端...";
    try {
      const result = await backendStatusWithTimeout();
      if (seq !== backendStatusCheckSeq) return;
      root.dataset.status = result.status === "ok" ? "connected" : "unknown";
      message.textContent = formatBackendStatusMessage(result);
      maybeRecoverBackendBridge(result);
      if (result.status !== "ok") {
        reportRendererEvent("backend_status_error", {
          status: result.status || "unknown",
          message: result.message || ""
        });
      }
    } catch (error) {
      if (seq !== backendStatusCheckSeq) return;
      root.dataset.status = "unknown";
      message.textContent = String(error);
      consecutiveBackendTimeouts = 0;
      reportRendererEvent("backend_status_error", { message: String(error) });
    }
  }

  function scheduleBackendStatusHeartbeat(root, message) {
    refreshBackendStatus(root, message);
    if (typeof window.setInterval !== "function" || window.__codexPilotBackendStatusHeartbeat) return;
    window.__codexPilotBackendStatusHeartbeat = window.setInterval(() => {
      if (!isActiveBoot() || !root.isConnected) {
        window.clearInterval?.(window.__codexPilotBackendStatusHeartbeat);
        delete window.__codexPilotBackendStatusHeartbeat;
        return;
      }
      refreshBackendStatus(root, message);
    }, 5000);
  }

  function normalizeText(value) {
    return String(value || "").replace(/\s+/g, " ").trim();
  }

  function createMenu() {
    if (!enhancementSettings.enabled) {
      return;
    }
    if (document.getElementById(rootId)) {
      return;
    }
    ensureStyles();

    const root = document.createElement("div");
    root.id = rootId;
    root.dataset.open = "false";
    root.dataset.status = "checking";

    const panel = document.createElement("div");
    panel.className = "codex-pilot-panel";

    const title = document.createElement("div");
    title.className = "codex-pilot-title";
    const versionLabel = scriptVersion && !scriptVersion.includes("__")
      ? scriptVersion
      : "dev";
    title.innerHTML = `<strong>CodexPilot</strong><span class="codex-pilot-version">${versionLabel}</span>`;

    const exportShell = document.createElement("div");
    exportShell.className = "codex-pilot-export-shell";
    const exportSplit = document.createElement("div");
    exportSplit.className = "codex-pilot-export-split";
    exportSplit.setAttribute("aria-label", "导出格式");
    const exportMarkdownButton = document.createElement("button");
    exportMarkdownButton.className = "codex-pilot-export-option";
    exportMarkdownButton.type = "button";
    setIconButtonContent(
      exportMarkdownButton,
      "导出 Markdown",
      '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><path d="M14 2v6h6"></path>'
    );
    const exportMarkdownLabel = document.createElement("span");
    exportMarkdownLabel.textContent = "导出 MD";
    exportMarkdownButton.append(exportMarkdownLabel);
    const exportHtmlButton = document.createElement("button");
    exportHtmlButton.className = "codex-pilot-export-option";
    exportHtmlButton.type = "button";
    setIconButtonContent(
      exportHtmlButton,
      "导出 HTML",
      '<path d="M3 5h18"></path><path d="M3 19h18"></path><path d="m9 9-3 3 3 3"></path><path d="m15 9 3 3-3 3"></path>'
    );
    const exportHtmlLabel = document.createElement("span");
    exportHtmlLabel.textContent = "导出 HTML";
    exportHtmlButton.append(exportHtmlLabel);
    exportSplit.append(exportMarkdownButton, exportHtmlButton);
    exportShell.append(exportSplit);

    const message = document.createElement("div");
    message.className = "codex-pilot-message";
    message.textContent = "正在检查后端...";

    exportMarkdownButton.addEventListener("click", async () => {
      const session = window.__CODEX_PILOT__.detectSession();
      if (!session?.session_id) {
        message.textContent = "未识别到会话，请先在左侧选择一个对话";
        return;
      }
      message.textContent = "正在导出 Markdown...";
      try {
        await exportSession(session, (text) => {
          message.textContent = text;
        });
      } catch (error) {
        message.textContent = String(error);
        reportRendererEvent("export_markdown_error", {
          message: String(error),
          session_id: session.session_id
        });
      }
    });

    exportHtmlButton.addEventListener("click", async () => {
      const session = window.__CODEX_PILOT__.detectSession();
      if (!session?.session_id) {
        message.textContent = "未识别到会话，请先在左侧选择一个对话";
        return;
      }
      message.textContent = "正在导出 HTML...";
      try {
        await exportSessionHtml(session, (text) => {
          message.textContent = text;
        });
      } catch (error) {
        message.textContent = String(error);
        reportRendererEvent("export_html_error", {
          message: String(error),
          session_id: session.session_id
        });
      }
    });

    const pill = document.createElement("div");
    pill.className = "codex-pilot-pill";

    const fastToggle = document.createElement("button");
    fastToggle.className = "codex-pilot-fast-toggle";
    fastToggle.type = "button";
    fastToggle.dataset.mode = "standard";
    fastToggle.dataset.source = "none";
    fastToggle.title = "当前对话使用 Standard";
    fastToggle.setAttribute("aria-label", "当前对话使用 Standard");
    fastToggle.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true" fill="currentColor"><path d="M13 2 4 14h6l-1 8 9-12h-6z"/></svg>';
    const fastLabel = document.createElement("span");
    fastLabel.className = "codex-pilot-fast-label";
    fastLabel.textContent = "";
    fastToggle.appendChild(fastLabel);
    fastToggle.addEventListener("click", (event) => {
      stopRowActionEvent(event);
      toggleCurrentServiceTierMode();
    }, true);
    fastToggleButton = fastToggle;
    fastToggleLabel = fastLabel;

    const toggle = document.createElement("button");
    toggle.className = "codex-pilot-button";
    toggle.type = "button";
    const statusDot = document.createElement("span");
    statusDot.className = "codex-pilot-status-dot";
    statusDot.setAttribute("aria-hidden", "true");
    const toggleLabel = document.createElement("span");
    toggleLabel.textContent = "Pilot";
    toggle.append(statusDot, toggleLabel);
    toggle.addEventListener("click", () => {
      root.dataset.open = root.dataset.open === "true" ? "false" : "true";
    });

    panel.append(title, exportShell, message);
    pill.append(fastToggle, toggle);
    root.append(panel, pill);
    document.body.appendChild(root);
    refreshFastToggleUi();
    scheduleBackendStatusHeartbeat(root, message);
  }

  function startRefreshLoop() {
    applyPluginPatches();
    if (enhancementSettings.inlineActions) refreshSessionActions();
    installRouteChangeTracking();
    if (enhancementSettings.scrollRestore) installScrollRestore();
    if (enhancementSettings.timeline) refreshTimelineSoon();
    installServiceTierDispatcherPatch();
    installServiceTierTestApi();
    refreshFastToggleUi();
    if (typeof MutationObserver === "function") {
      // 这些处理器自身会改 body 子树（插入时间线 / 行内操作 / fast 按钮等），而 observer 又监听整棵
      // body 子树的 childList。若在回调里同步跑处理器，我们自己造成的变更会立刻再次触发 observer，
      // 形成自反馈死循环、渲染进程 100% CPU 卡死（新建对话冷启动时必现）。这里两道防线：
      //   ① 合并一帧内的多次变更，最多跑一次（debounce）；
      //   ② 跑处理器期间先 disconnect，使我们自己造成的变更不被观测，从根上切断自反馈。
      // 注意时间线用 renderTimeline() 直接同步渲染，而不是 refreshTimelineSoon() 的延时渲染，
      // 以保证所有 DOM 变更都落在 disconnect 窗口内。
      let refreshScheduled = false;
      const observer = new MutationObserver(() => {
        if (!isActiveBoot()) {
          observer.disconnect();
          return;
        }
        if (refreshScheduled) return;
        refreshScheduled = true;
        const runRefresh = () => {
          refreshScheduled = false;
          if (!isActiveBoot()) {
            observer.disconnect();
            return;
          }
          observer.disconnect();
          try {
            applyPluginPatches();
            if (enhancementSettings.inlineActions) refreshSessionActions();
            if (enhancementSettings.timeline) renderTimeline();
            refreshFastToggleUi();
          } finally {
            if (isActiveBoot()) {
              observer.observe(document.body, { childList: true, subtree: true });
            }
          }
        };
        // 用 ~250ms 节流而非逐帧（rAF）：Codex 自身的动画元素每帧都在变动，逐帧跑处理器会让
        // 冷启动期 CPU 飙到接近满载、稳态也无谓地每帧重写 fast 按钮。250ms 反应延迟无感，CPU 大降。
        window.setTimeout(runRefresh, 250);
      });
      observer.observe(document.body, { childList: true, subtree: true });
    }
    if (typeof window.setInterval === "function") {
      window.setInterval(() => {
        if (!isActiveBoot()) return;
        applyPluginPatches();
        if (enhancementSettings.inlineActions) refreshSessionActions();
        if (enhancementSettings.timeline) renderTimeline();
        if (fastPendingBindStartedAt && Date.now() - fastPendingBindStartedAt > serviceTierDraftTtlMs) {
          fastPendingBindStartedAt = 0;
          clearServiceTierDraft("pending_bind_ttl");
        }
        refreshFastToggleUi();
        installServiceTierDispatcherPatch();
      }, 1500);
    }
  }

  async function bootCodexPilot() {
    await loadEnhancementSettings();
    if (!isActiveBoot()) {
      return;
    }
    if (!enhancementSettings.enabled) {
      await uninstallServiceTierDispatcherPatch("enhancement_disabled");
      reportRendererEvent("enhancement_disabled", {});
      return;
    }
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", () => {
        createMenu();
        startRefreshLoop();
      }, { once: true });
    } else {
      createMenu();
      startRefreshLoop();
    }
    reportRendererEvent("loaded", {
      helper_port: helperPort,
      enhancement_settings: enhancementSettings
    });
  }

  bootCodexPilot();
  console.info("[CodexPilot] renderer script loaded", window.__CODEX_PILOT__);
})();
