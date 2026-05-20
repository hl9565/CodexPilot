(function () {
  const scriptVersion = "__CODEX_PILOT_VERSION__";
  if (window.__CODEX_PILOT_INJECTED__ === scriptVersion) {
    return;
  }
  const existingRoot = document.getElementById("codex-pilot-root");
  if (existingRoot) {
    existingRoot.remove();
  }
  window.__CODEX_PILOT_INJECTED__ = scriptVersion;

  const helperPort = Number("__CODEX_PILOT_HELPER_PORT__");
  const rootId = "codex-pilot-root";
  const actionGroupClass = "codex-pilot-row-actions";
  const actionButtonClass = "codex-pilot-row-action";
  const archiveActionClass = "codex-pilot-archive-action";
  const selectors = {
    sidebarThread: "[data-app-action-sidebar-thread-id]",
    threadTitle: "[data-thread-title], .truncate.select-none, .truncate.text-base",
    archiveNav: 'button[aria-label="已归档对话"], button[aria-label="Archived conversations"]'
  };
  let lastUndoToken = null;

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
    detectSession() {
      return detectCurrentSession();
    },
    exportMarkdown(session) {
      return this.bridge("/session/export-markdown", session);
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

  function ensureStyles() {
    if (document.getElementById("codex-pilot-style")) {
      return;
    }
    const style = document.createElement("style");
    style.id = "codex-pilot-style";
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
        background: #2563eb;
        border: 1px solid #1d55d7;
        border-radius: 999px;
        box-shadow: 0 10px 28px rgba(15, 23, 42, 0.22);
        color: #ffffff;
        cursor: pointer;
        display: inline-flex;
        font-size: 13px;
        font-weight: 700;
        gap: 8px;
        min-height: 38px;
        padding: 0 14px;
      }

      #${rootId} .codex-pilot-panel {
        background: #ffffff;
        border: 1px solid #d7dde5;
        border-radius: 8px;
        bottom: 48px;
        box-shadow: 0 18px 42px rgba(15, 23, 42, 0.22);
        display: none;
        min-width: 238px;
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

      #${rootId} .codex-pilot-pill {
        background: #eef4ff;
        border-radius: 999px;
        color: #17408f;
        font-size: 11px;
        font-weight: 700;
        padding: 3px 7px;
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
        display: inline-flex;
        gap: 4px;
        opacity: 0;
        pointer-events: none;
        position: absolute;
        right: 8px;
        top: 50%;
        transform: translateY(-50%);
        transition: opacity 120ms ease;
        z-index: 4;
      }

      [data-codex-pilot-row="true"]:hover .${actionGroupClass},
      [data-codex-pilot-row="true"]:focus-within .${actionGroupClass} {
        opacity: 1;
        pointer-events: auto;
      }

      .${actionButtonClass},
      .${archiveActionClass} {
        background: #ffffff;
        border: 1px solid #d7dde5;
        border-radius: 6px;
        color: #233044;
        cursor: pointer;
        font-size: 12px;
        font-weight: 650;
        line-height: 1;
        min-height: 26px;
        padding: 0 8px;
        white-space: nowrap;
      }

      .${actionButtonClass}:hover,
      .${archiveActionClass}:hover {
        background: #eef4ff;
        border-color: #b9cdf8;
      }

      .${actionButtonClass}[data-danger="true"],
      .${archiveActionClass}[data-danger="true"] {
        color: #9f1d1d;
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
    `;
    document.head.appendChild(style);
  }

  function detectCurrentSession() {
    const byUrl = sessionRefFromUrl();
    const bySelectedRow = sessionRefFromSelectedRow();
    const byVisibleRow = sessionRefFromVisibleRows(byUrl?.session_id);
    return bySelectedRow || byVisibleRow || byUrl || null;
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
    const patterns = [
      /(?:session|conversation|thread)[=/:-]([A-Za-z0-9_.-]{8,})/i,
      /\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})(?:[/?#]|$)/i,
      /\/([A-Za-z0-9_-]{12,})(?:[/?#]|$)/
    ];
    for (const pattern of patterns) {
      const match = href.match(pattern);
      if (match?.[1]) {
        return { session_id: match[1], title: document.title || "当前会话", source: "url" };
      }
    }
    return null;
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

  function stopRowActionEvent(event) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation?.();
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

  async function deleteSession(session, row, notify = showToast) {
    const title = session.title || session.session_id;
    if (!window.confirm(`确认删除“${title}”？删除前会创建可撤销备份。`)) {
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
    return true;
  }

  function attachRowActions(row) {
    if (!row || row.querySelector(`.${actionGroupClass}`)) return;
    const session = sessionRefFromRow(row, "row");
    if (!session?.session_id) return;
    row.dataset.codexPilotRow = "true";
    const group = document.createElement("div");
    group.className = actionGroupClass;

    const exportButton = document.createElement("button");
    exportButton.type = "button";
    exportButton.className = actionButtonClass;
    exportButton.textContent = "导出";
    exportButton.addEventListener("click", async (event) => {
      stopRowActionEvent(event);
      await exportSession(session);
    }, true);

    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.className = actionButtonClass;
    deleteButton.dataset.danger = "true";
    deleteButton.textContent = "删除";
    deleteButton.addEventListener("click", async (event) => {
      stopRowActionEvent(event);
      await deleteSession(session, row);
    }, true);

    group.append(exportButton, deleteButton);
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

    const exportButton = document.createElement("button");
    exportButton.type = "button";
    exportButton.className = archiveActionClass;
    exportButton.textContent = "导出";
    exportButton.addEventListener("click", async (event) => {
      stopRowActionEvent(event);
      const session = await resolveArchiveSession(row);
      if (!session.session_id) {
        showToast("导出失败：未找到归档会话 ID", null);
        return;
      }
      await exportSession(session);
    }, true);

    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.className = archiveActionClass;
    deleteButton.dataset.danger = "true";
    deleteButton.textContent = "删除";
    deleteButton.addEventListener("click", async (event) => {
      stopRowActionEvent(event);
      const session = await resolveArchiveSession(row);
      if (!session.session_id) {
        showToast("删除失败：未找到归档会话 ID", null);
        return;
      }
      await deleteSession(session, row);
    }, true);

    unarchiveButton.insertAdjacentElement("afterend", deleteButton);
    unarchiveButton.insertAdjacentElement("afterend", exportButton);
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
      let deleted = 0;
      for (const row of currentRows) {
        const session = await resolveArchiveSession(row);
        if (!session.session_id) continue;
        const response = await window.__CODEX_PILOT__.deleteSession(sessionPayload(session));
        const result = response.result || response;
        if (response.status === "ok" && result.status !== "failed" && result.status !== "not_found") {
          row.remove();
          deleted += 1;
        }
      }
      showToast(`已删除 ${deleted} 个归档会话`, null);
    }, true);
    const heading = Array.from(document.querySelectorAll("h1, h2, h3"))
      .find((element) => ["已归档对话", "Archived conversations"].includes(normalizeText(element.textContent)));
    (heading || document.body).appendChild(button);
  }

  function refreshSessionActions() {
    sessionRows().forEach(attachRowActions);
    const rows = archiveRows();
    rows.forEach(attachArchiveActions);
    installArchiveDeleteAll(rows);
  }

  function normalizeText(value) {
    return String(value || "").replace(/\s+/g, " ").trim();
  }

  function createMenu() {
    if (document.getElementById(rootId)) {
      return;
    }
    ensureStyles();

    const root = document.createElement("div");
    root.id = rootId;
    root.dataset.open = "false";

    const panel = document.createElement("div");
    panel.className = "codex-pilot-panel";

    const title = document.createElement("div");
    title.className = "codex-pilot-title";
    title.innerHTML = `<strong>CodexPilot</strong><span class="codex-pilot-pill">${scriptVersion}</span>`;

    const statusButton = document.createElement("button");
    statusButton.className = "codex-pilot-action";
    statusButton.type = "button";
    statusButton.innerHTML = "<span>后端状态</span><span>检查</span>";

    const exportButton = document.createElement("button");
    exportButton.className = "codex-pilot-action";
    exportButton.type = "button";
    exportButton.innerHTML = "<span>导出 Markdown</span><span>导出</span>";

    const message = document.createElement("div");
    message.className = "codex-pilot-message";
    message.textContent = "就绪";

    statusButton.addEventListener("click", async () => {
      message.textContent = "正在检查后端...";
      try {
        const result = await window.__CODEX_PILOT__.backendStatus();
        message.textContent = result.status === "ok"
          ? `${result.message || "后端已连接"} (${result.transport || "bridge"})`
          : result.message || "后端检查失败";
      } catch (error) {
        message.textContent = String(error);
        reportRendererEvent("backend_status_error", { message: String(error) });
      }
    });

    exportButton.addEventListener("click", async () => {
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

    const toggle = document.createElement("button");
    toggle.className = "codex-pilot-button";
    toggle.type = "button";
    toggle.textContent = "助手";
    toggle.addEventListener("click", () => {
      root.dataset.open = root.dataset.open === "true" ? "false" : "true";
    });

    panel.append(title, statusButton, exportButton, message);
    root.append(panel, toggle);
    document.body.appendChild(root);
  }

  function startRefreshLoop() {
    refreshSessionActions();
    if (typeof MutationObserver === "function") {
      const observer = new MutationObserver(() => refreshSessionActions());
      observer.observe(document.body, { childList: true, subtree: true });
    }
    if (typeof window.setInterval === "function") {
      window.setInterval(refreshSessionActions, 1500);
    }
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

  reportRendererEvent("loaded", { helper_port: helperPort });
  console.info("[CodexPilot] renderer script loaded", window.__CODEX_PILOT__);
})();
