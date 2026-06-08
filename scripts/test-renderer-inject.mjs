import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

class FixtureURL extends URL {
  static createObjectURL() {
    return "blob:codex-pilot-test";
  }

  static revokeObjectURL() {}
}

class MiniElement {
  constructor(tagName) {
    this.tagName = tagName.toLowerCase();
    this.attributes = new Map();
    this.children = [];
    this.parentElement = null;
    this.dataset = {};
    this.eventListeners = new Map();
    this._className = "";
    this.disabled = false;
    this._id = "";
    this.title = "";
    this.style = {};
    this.isConnected = true;
    this.offsetTop = 0;
    this.scrollTop = 0;
    this.scrollHeight = 1200;
    this.clientHeight = 420;
    this._innerHTML = "";
    this._textContent = "";
  }

  setAttribute(name, value) {
    const text = String(value);
    this.attributes.set(name, text);
    if (name === "id") this.id = text;
    if (name === "class") this.className = text;
  }

  getAttribute(name) {
    return this.attributes.get(name) ?? null;
  }

  set id(value) {
    this._id = String(value);
    if (this._id) {
      this.attributes.set("id", this._id);
    } else {
      this.attributes.delete("id");
    }
  }

  get id() {
    return this._id;
  }

  set className(value) {
    this._className = String(value);
    if (this._className) {
      this.attributes.set("class", this._className);
    } else {
      this.attributes.delete("class");
    }
  }

  get className() {
    return this._className;
  }

  append(...nodes) {
    for (const node of nodes) {
      this.appendChild(node);
    }
  }

  appendChild(node) {
    node.parentElement = this;
    node.isConnected = true;
    this.children.push(node);
    return node;
  }

  remove() {
    if (!this.parentElement) return;
    const siblings = this.parentElement.children;
    const index = siblings.indexOf(this);
    if (index >= 0) siblings.splice(index, 1);
    this.parentElement = null;
    this.isConnected = false;
  }

  addEventListener(type, handler) {
    const handlers = this.eventListeners.get(type) || [];
    handlers.push(handler);
    this.eventListeners.set(type, handlers);
  }

  async click() {
    this.clicked = true;
    const handlers = this.eventListeners.get("click") || [];
    const event = {
      target: this,
      preventDefault() {},
      stopPropagation() {},
      stopImmediatePropagation() {}
    };
    await Promise.all(handlers.map((handler) => handler(event)));
  }

  querySelector(selector) {
    return this.querySelectorAll(selector)[0] || null;
  }

  querySelectorAll(selector) {
    const selectors = selector.split(",").map((item) => item.trim());
    const found = [];
    const visit = (node) => {
      if (selectors.some((item) => node.matches(item))) {
        found.push(node);
      }
      for (const child of node.children) visit(child);
    };
    for (const child of this.children) visit(child);
    return found;
  }

  closest(selector) {
    let current = this;
    while (current) {
      if (current.matches(selector)) return current;
      current = current.parentElement;
    }
    return null;
  }

  contains(node) {
    if (node === this) return true;
    return this.children.some((child) => child.contains(node));
  }

  matches(selector) {
    if (selector === this.tagName) return true;
    if (selector === `#${this.id}`) return true;
    if (selector.includes(",")) {
      return selector.split(",").some((item) => this.matches(item.trim()));
    }
    if (selector.startsWith(".")) {
      const className = selector.slice(1);
      return this.className.split(/\s+/).includes(className);
    }
    if (selector === "li") {
      return this.tagName === "li";
    }
    if (selector === "[role='listitem']") {
      return this.getAttribute("role") === "listitem";
    }
    if (selector === "[data-app-action-sidebar-thread-id]") {
      return Boolean(this.getAttribute("data-app-action-sidebar-thread-id"));
    }
    if (selector === "[data-thread-title]") {
      return this.attributes.has("data-thread-title");
    }
    if (selector === "[data-testid*='thread']") {
      return String(this.getAttribute("data-testid") || "").includes("thread");
    }
    if (selector === "[data-message-author-role='user']") {
      return this.getAttribute("data-message-author-role") === "user";
    }
    if (selector === "[data-testid*='conversation-turn']") {
      return String(this.getAttribute("data-testid") || "").includes("conversation-turn");
    }
    if (selector === "[data-testid*='user-message']") {
      return String(this.getAttribute("data-testid") || "").includes("user-message");
    }
    if (selector === "[class*='user-message']") {
      return this.className.includes("user-message");
    }
    return false;
  }

  getBoundingClientRect() {
    return {
      width: 180,
      height: this.hidden ? 0 : 32,
      top: this.offsetTop || 0
    };
  }

  scrollIntoView(options) {
    this.scrolledIntoView = options || true;
  }

  set innerHTML(value) {
    this._innerHTML = String(value);
    this._textContent = this._innerHTML.replace(/<[^>]*>/g, "");
    this.children = [];
    if (/<svg[\s>]/.test(this._innerHTML)) {
      const svg = new MiniElement("svg");
      svg.parentElement = this;
      this.children.push(svg);
    }
  }

  get innerHTML() {
    return this._innerHTML;
  }

  set textContent(value) {
    this._textContent = String(value);
  }

  get textContent() {
    if (this._textContent) return this._textContent;
    return this.children.map((child) => child.textContent).join("");
  }
}

class MiniDocument {
  constructor() {
    this.readyState = "complete";
    this.head = new MiniElement("head");
    this.body = new MiniElement("body");
    this.documentElement = new MiniElement("html");
    this.scrollingElement = this.body;
    this.title = "Codex 测试窗口";
  }

  createElement(tagName) {
    return new MiniElement(tagName);
  }

  getElementById(id) {
    return this.querySelector(`#${id}`);
  }

  querySelector(selector) {
    return this.querySelectorAll(selector)[0] || null;
  }

  querySelectorAll(selector) {
    return [...this.head.querySelectorAll(selector), ...this.body.querySelectorAll(selector)];
  }

  addEventListener() {}
}

function makeThreadRow(id, title, selected = false) {
  const listItem = new MiniElement("li");
  listItem.setAttribute("role", "listitem");
  const row = new MiniElement("button");
  row.setAttribute("data-app-action-sidebar-thread-id", id);
  if (selected) row.setAttribute("aria-current", "page");
  const titleNode = new MiniElement("span");
  titleNode.setAttribute("data-thread-title", "");
  titleNode.textContent = title;
  row.append(titleNode);
  listItem.append(row);
  return { listItem, row };
}

const source = fs.readFileSync(new URL("../assets/inject/renderer-inject.js", import.meta.url), "utf8");

function makeMessage({ text, role = "user", testId = "", className = "", offsetTop = 0 }) {
  const message = new MiniElement("article");
  if (role) message.setAttribute("data-message-author-role", role);
  if (testId) message.setAttribute("data-testid", testId);
  if (className) message.className = className;
  message.offsetTop = offsetTop;
  message.textContent = text;
  return message;
}

function createFixture({
  backendStatusMode = "ok",
  includeOther = true,
  messages,
  url = "https://chatgpt.com/codex",
  delaySettingStorageImport = false,
  enhancementSettings = {}
} = {}) {
  const document = new MiniDocument();
  const selected = makeThreadRow("thread-selected-12345", "测试对话", true);
  const other = includeOther ? makeThreadRow("thread-other-12345", "其他对话", false) : null;
  const threadMessages = messages || [
    makeMessage({ text: "请帮我解释这段代码", offsetTop: 120 }),
    makeMessage({ text: "再帮我补一个测试", offsetTop: 760 })
  ];
  document.body.append(...[selected.listItem, other?.listItem, ...threadMessages].filter(Boolean));

  const bridgeCalls = [];
  const navigationStateByCall = [];
  const confirmMessages = [];
  const intervals = [];
  const storage = new Map();
  const mutationObservers = [];
  const navigationClicks = [];
  const timeoutQueue = [];
  const dispatchedMessages = [];
  const dispatcher = {
    dispatchMessage(type, payload) {
      dispatchedMessages.push({ type, payload });
      return { type, payload };
    }
  };
  let releaseSettingStorageImport = null;
  let settingStorageImportStarted = 0;
  const settingStorageModule = {
    v: class FixtureDispatcher {
      static getInstance() {
        return dispatcher;
      }

      dispatchMessage() {}
    }
  };
  class FixtureMutationObserver {
    constructor(callback) {
      this.callback = callback;
      mutationObservers.push(this);
    }

    observe() {}

    trigger() {
      this.callback([]);
    }
  }
  const context = {
    console: { info() {} },
    performance: {
      getEntriesByType(type) {
        return type === "resource"
          ? [{ name: "https://chatgpt.com/assets/setting-storage-fixture.js" }]
          : [];
      }
    },
    setTimeout(callback, delay = 0) {
      if (typeof callback === "function" && Number(delay) < 1000) callback();
      return 1;
    },
    Blob: class {},
    MutationObserver: FixtureMutationObserver,
    URL: FixtureURL,
    document,
    history: {
      pushState() {},
      replaceState() {}
    },
    window: {
      __CODEX_PILOT_TEST__: true,
      __CODEX_PILOT_TEST_LOAD_CODEX_APP_MODULE__(namePart) {
        assert.equal(namePart, "setting-storage-", "Fast dispatcher patch 应加载 Codex setting storage 模块");
        settingStorageImportStarted += 1;
        if (delaySettingStorageImport) {
          return new Promise((resolve) => {
            releaseSettingStorageImport = () => resolve(settingStorageModule);
          });
        }
        return Promise.resolve(settingStorageModule);
      },
      location: {
        href: url,
        reloadCalled: false,
        reload() {
          this.reloadCalled = true;
        }
      },
      innerHeight: 420,
      scrollY: 0,
      setTimeout(callback, delay = 0) {
        if (typeof callback === "function" && Number(delay) < 1000) {
          callback();
        } else if (typeof callback === "function") {
          timeoutQueue.push({ callback, delay });
        }
        return 1;
      },
      clearTimeout() {},
      setInterval(callback, delay = 0) {
        intervals.push({ callback, delay });
        return intervals.length;
      },
      scrollTo(options) {
        this.scrollY = typeof options === "object" ? options.top : Number(options) || 0;
      },
      addEventListener() {},
      localStorage: {
        getItem(key) {
          return storage.get(key) ?? null;
        },
        setItem(key, value) {
          storage.set(key, String(value));
        },
        removeItem(key) {
          storage.delete(key);
        }
      },
      confirm(message) {
        confirmMessages.push(message);
        return true;
      },
      __codexPilotBridge(path, payload) {
        bridgeCalls.push({ path, payload });
        navigationStateByCall.push({ path, navigationClicks: [...navigationClicks] });
        if (path === "/session/export-markdown") {
          return Promise.resolve({
            status: "ok",
            result: {
              status: "exported",
              filename: "测试对话.md",
              markdown: "# 测试对话"
            }
          });
        }
        if (path === "/session/export-html") {
          return Promise.resolve({
            status: "ok",
            result: {
              status: "exported",
              filename: "测试对话.html",
              html: "<!doctype html><title>测试对话</title>"
            }
          });
        }
        if (path === "/session/delete") {
          return Promise.resolve({
            status: "ok",
            result: {
              status: "deleted",
              message: "已删除本地会话",
              undo_token: "undo-token-1"
            }
          });
        }
        if (path === "/enhancement/settings") {
          return Promise.resolve({
            status: "ok",
            result: {
              enabled: true,
              timeline: true,
              inlineActions: true,
              scrollRestore: true,
              ...enhancementSettings
            }
          });
        }
        if (path === "/backend/status" && backendStatusMode === "timeout") {
          return new Promise(() => {});
        }
        if (path === "/backend/recover-bridge") {
          return Promise.resolve({
            status: "ok",
            message: "CodexPilot bridge 已重新注入"
          });
        }
        if (path === "/session/undo") {
          return Promise.resolve({
            status: "ok",
            result: {
              status: "undone",
              message: "已撤销删除"
            }
          });
        }
        return Promise.resolve({ status: "ok", message: "后端已连接" });
      }
    }
  };
  selected.row.addEventListener("click", () => {
    selected.row.setAttribute("aria-current", "page");
    if (other?.row) other.row.attributes.delete("aria-current");
    context.window.location.href = `https://chatgpt.com/codex/${selected.row.getAttribute("data-app-action-sidebar-thread-id")}`;
    navigationClicks.push(selected.row.getAttribute("data-app-action-sidebar-thread-id"));
  });
  if (other?.row) {
    other.row.addEventListener("click", () => {
      other.row.setAttribute("aria-current", "page");
      selected.row.attributes.delete("aria-current");
      context.window.location.href = `https://chatgpt.com/codex/${other.row.getAttribute("data-app-action-sidebar-thread-id")}`;
      navigationClicks.push(other.row.getAttribute("data-app-action-sidebar-thread-id"));
    });
  }
  context.window.window = context.window;
  context.window.document = document;
  context.window.history = context.history;
  context.window.performance = context.performance;
  vm.runInNewContext(source, context, { filename: "renderer-inject.js" });
  return {
    bridgeCalls,
    confirmMessages,
    context,
    dispatchedMessages,
    dispatcher,
    document,
    intervals,
    messages: threadMessages,
    mutationObservers,
    navigationClicks,
    navigationStateByCall,
    other,
    releaseSettingStorageImport: () => releaseSettingStorageImport?.(),
    selected,
    settingStorageImportStarted: () => settingStorageImportStarted,
    storage,
    timeoutQueue
  };
}

async function deleteSelected(fixture) {
  await flushAsyncWork();
  const rowDeleteButton = fixture.selected.row.querySelectorAll("button")
    .find((button) => button.getAttribute("aria-label") === "删除会话");
  assert.ok(rowDeleteButton, "应在会话行添加删除按钮");
  await rowDeleteButton.click();
  return rowDeleteButton;
}

async function flushAsyncWork() {
  for (let index = 0; index < 8; index += 1) {
    await Promise.resolve();
  }
}

{
  const fixture = createFixture();
  await flushAsyncWork();
  const { bridgeCalls, confirmMessages, document, intervals, messages, other, selected } = fixture;
  const root = document.getElementById("codex-pilot-root");
  assert.ok(root, "应创建 CodexPilot 浮动菜单");
  assert.equal(root.dataset.status, "connected");
  assert.match(root.textContent, /Pilot|导出 MD|导出 HTML/);
  assert.doesNotMatch(root.textContent, /后端状态|检查/);
  assert.match(root.textContent, /CodexPilot|dev/);
  assert.doesNotMatch(root.textContent, /助手/);
  assert.ok(root.querySelector(".codex-pilot-status-dot"), "应显示 Pilot 状态点");
  assert.doesNotMatch(root.textContent, /当前会话|删除会话|撤销删除/);
  assert.ok(bridgeCalls.some((call) => call.path === "/backend/status"), "应自动检查后端状态");
  assert.ok(intervals.some((item) => item.delay === 5000), "应启动后端状态心跳");
  assert.ok(bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "loaded"));
  assert.ok(bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "scroll_restore_ready"));

  const buttons = root.querySelectorAll("button");
  const floatingExportButton = buttons.find((button) => button.getAttribute("aria-label") === "导出 Markdown");
  const floatingHtmlButton = buttons.find((button) => button.getAttribute("aria-label") === "导出 HTML");
  assert.ok(floatingExportButton, "浮窗应显示 Markdown 导出入口");
  assert.ok(floatingHtmlButton, "浮窗应显示 HTML 导出入口");
  const message = root.querySelector(".codex-pilot-message");
  assert.match(message.textContent, /后端已连接/);

  await floatingExportButton.click();
  const exportCall = bridgeCalls.find((call) => call.path === "/session/export-markdown");
  assert.equal(JSON.stringify(exportCall), JSON.stringify({
    path: "/session/export-markdown",
    payload: {
      id: "thread-selected-12345",
      session_id: "thread-selected-12345",
      title: "测试对话"
    }
  }));
  assert.equal(message.textContent, "已导出：测试对话.md");

  await floatingHtmlButton.click();
  const htmlExportCall = bridgeCalls.find((call) => call.path === "/session/export-html");
  assert.equal(JSON.stringify(htmlExportCall), JSON.stringify({
    path: "/session/export-html",
    payload: {
      id: "thread-selected-12345",
      session_id: "thread-selected-12345",
      title: "测试对话"
    }
  }));
  assert.equal(message.textContent, "已导出：测试对话.html");

  const rowDeleteButton = selected.row.querySelectorAll("button")
    .find((button) => button.getAttribute("aria-label") === "删除会话");
  assert.ok(rowDeleteButton, "应在会话行添加删除按钮");
  assert.equal(rowDeleteButton.title, "删除会话");
  const rowExportButton = selected.row.querySelectorAll("button")
    .find((button) => button.getAttribute("aria-label") === "导出 Markdown");
  assert.equal(rowExportButton, undefined, "会话行不再显示 Markdown 导出按钮");
  const rowActionGroup = selected.row.querySelector(".codex-pilot-row-actions");
  assert.ok(rowActionGroup, "应创建独立的会话行操作组");
  assert.equal(rowActionGroup.children.length, 1, "会话行操作组只包含删除按钮");
  const styleText = document.getElementById("codex-pilot-style").textContent;
  const rowActionsStyle = styleText.match(/\.codex-pilot-row-actions\s*\{[^}]+\}/)?.[0] || "";
  assert.match(rowActionsStyle, /right:\s*140px;/, "会话行操作组应落在原导出按钮位置，避开 Codex 原生右侧按钮");
  assert.doesNotMatch(rowActionsStyle, /left:\s*\d+px;/, "会话行操作组不应同时保留左侧定位");
  assert.match(
    styleText,
    /mask-image:\s*linear-gradient\(90deg,\s*#000 calc\(100% - 132px\),\s*transparent calc\(100% - 116px\)/,
    "悬停时应遮罩标题右侧，避免文字与操作按钮重叠"
  );

  const timeline = document.getElementById("codex-pilot-timeline");
  assert.ok(timeline, "应为长对话创建时间线");
  const markers = timeline.querySelectorAll(".codex-pilot-timeline-marker");
  assert.equal(markers.length, 2, "应为两个用户问题创建时间线标记");
  assert.equal(markers[0].querySelector(".codex-pilot-timeline-tooltip").textContent, "请帮我解释这段代码");
  await markers[0].click();
  assert.ok(messages[0].scrolledIntoView, "点击时间线标记应滚动到对应消息");
  assert.ok(bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "timeline_jump"));

  await rowDeleteButton.click();
  assert.deepEqual(confirmMessages, [
    "确认删除“测试对话”？删除前会创建可撤销备份。",
    "你正在删除当前打开的会话。删除成功后会刷新页面，确认继续？"
  ]);
  const deleteCall = bridgeCalls.find((call) => call.path === "/session/delete");
  assert.equal(JSON.stringify(deleteCall), JSON.stringify({
    path: "/session/delete",
    payload: {
      id: "thread-selected-12345",
      session_id: "thread-selected-12345",
      title: "测试对话"
    }
  }));
  assert.equal(selected.listItem.parentElement, null, "删除成功后应同步移除侧边栏行");
  assert.equal(other.listItem.parentElement, document.body, "其他会话不能被误删");
  const toast = document.body.querySelector(".codex-pilot-toast");
  assert.ok(toast, "删除成功后应显示 Toast");
  assert.match(toast.textContent, /已删除本地会话|撤销/);
  assert.equal(fixture.context.window.location.reloadCalled, true, "删除当前会话后应刷新页面");

  const undoButton = toast.querySelector("button");
  assert.ok(undoButton, "Toast 应提供撤销按钮");
  await undoButton.click();
  const undoCall = bridgeCalls.find((call) => call.path === "/session/undo");
  assert.equal(JSON.stringify(undoCall), JSON.stringify({
    path: "/session/undo",
    payload: { undo_token: "undo-token-1" }
  }));
  assert.equal(toast.textContent, "已撤销删除");
}

{
  const fixture = createFixture({ includeOther: false });
  await deleteSelected(fixture);
  assert.equal(fixture.selected.listItem.parentElement, null, "删除成功后应移除唯一会话行");
  assert.equal(fixture.context.window.location.reloadCalled, true, "删除唯一会话后应刷新页面");
}

{
  const fixture = createFixture({
    messages: [
      makeMessage({ text: "只有一个问题", offsetTop: 120 })
    ]
  });
  await flushAsyncWork();
  assert.equal(fixture.document.getElementById("codex-pilot-timeline"), null, "只有一个用户问题时不应显示时间线");
  assert.ok(fixture.bridgeCalls.some((call) => call.payload?.event === "timeline_no_targets"));
}

{
  const fixture = createFixture({ backendStatusMode: "timeout" });
  await flushAsyncWork();
  const heartbeat = fixture.intervals.find((item) => item.delay === 5000);
  assert.ok(heartbeat, "应启动后端状态心跳");
  for (let index = 0; index < 3; index += 1) {
    if (fixture.timeoutQueue.length === 0) {
      heartbeat.callback();
      await flushAsyncWork();
    }
    const timeout = fixture.timeoutQueue.shift();
    assert.ok(timeout, "应登记后端状态超时定时器");
    timeout.callback();
    await flushAsyncWork();
    if (index < 2) {
      heartbeat.callback();
      await flushAsyncWork();
    }
  }
  assert.ok(
    fixture.bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "backend_recovery_requested"),
    "连续超时后应记录恢复请求"
  );
  assert.ok(
    fixture.bridgeCalls.some((call) => call.path === "/backend/recover-bridge"),
    "连续超时后应请求重新注入 bridge"
  );
  assert.ok(
    fixture.bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "backend_recovery_result"),
    "恢复请求应写入结果诊断"
  );
}

{
  const fixture = createFixture({
    messages: [
      makeMessage({ text: "用户: 第一个问题", role: "", testId: "conversation-turn", offsetTop: 120 }),
      makeMessage({ text: "助手: 这是回答，不应该成为标记", role: "", testId: "conversation-turn", offsetTop: 420 }),
      makeMessage({ text: "用户: 第二个问题", role: "", testId: "conversation-turn", offsetTop: 760 })
    ]
  });
  await flushAsyncWork();
  const markers = fixture.document.querySelectorAll(".codex-pilot-timeline-marker");
  assert.equal(markers.length, 2, "conversation-turn fallback 应只保留用户轮次");
  assert.equal(markers[1].querySelector(".codex-pilot-timeline-tooltip").textContent, "用户: 第二个问题");
}

{
  const fixture = createFixture();
  await flushAsyncWork();
  const originalTimeline = fixture.document.getElementById("codex-pilot-timeline");
  assert.ok(originalTimeline, "初次渲染应创建时间线");
  fixture.mutationObservers.forEach((observer) => observer.trigger());
  const timelines = fixture.document.querySelectorAll("#codex-pilot-timeline");
  assert.equal(timelines.length, 1, "重复刷新不能创建多个时间线根节点");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  assert.ok(api, "测试环境应暴露 Fast 测试 API");
  api.clear();
  const root = fixture.document.getElementById("codex-pilot-root");
  const fastToggle = root.querySelector(".codex-pilot-fast-toggle");
  const panelToggle = root.querySelector(".codex-pilot-button");
  assert.ok(fastToggle, "Pilot pill 应显示 Fast 闪电按钮");
  const openBeforeFastClick = root.dataset.open;
  await fastToggle.click();
  assert.equal(root.dataset.open, openBeforeFastClick, "点击 Fast 不能打开 Pilot 面板");
  assert.notEqual(root.dataset.open, "true", "点击 Fast 后 Pilot 面板不应处于打开状态");
  assert.equal(fastToggle.dataset.mode, "fast", "新对话 draft 应显示 Fast");

  const start = api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { prompt: "hello" }
  });
  assert.equal(start.params.serviceTier, "priority", "Fast draft 应让新对话首请求使用 priority");
  const store = api.state();
  assert.equal(store.draft.pendingBind, true, "draft-backed 请求后应进入待绑定状态");
  assert.ok(store.draft.startToken, "待绑定 draft 应记录 start token");
  await panelToggle.click();
  assert.equal(root.dataset.open, "true", "Pilot 面板按钮仍应独立工作");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  await fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { prompt: "hello" }
  });
  fixture.context.window.location.href = "https://chatgpt.com/codex/thread-selected-12345";
  assert.equal(api.bind("test_old_thread", "thread-selected-12345"), false, "draft 不应绑定到发起前已存在的旧 thread");
  assert.ok(api.state().draft, "误入旧 thread 时 draft 应保留等待真正新 thread");

  fixture.context.window.location.href = "https://chatgpt.com/codex/thread-new-98765";
  assert.equal(api.bind("test_new_thread", "thread-new-98765"), true, "draft 应绑定到新出现的 thread id");
  assert.equal(api.state().entries["thread-new-98765"].mode, "fast", "绑定后新 thread 应保存 Fast 覆盖");
  assert.equal(api.state().draft, null, "成功绑定后应清理 draft");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  await fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { prompt: "first" }
  });
  const firstDraft = api.state().draft;
  assert.equal(firstDraft.existingSessionIds.includes("thread-new-98765"), false, "首个 start 前快照不包含新 thread");

  const newThread = makeThreadRow("thread-new-98765", "新对话", false);
  fixture.document.body.append(newThread.listItem);
  api.override({
    type: "thread-prewarm-start",
    request: {
      params: { prompt: "prewarm after row appears" }
    }
  });
  const secondDraft = api.state().draft;
  assert.equal(secondDraft.startToken, firstDraft.startToken, "pending draft 的 start token 不应被后续 envelope 覆盖");
  assert.equal(secondDraft.existingSessionIds.includes("thread-new-98765"), false, "pending draft 的旧 thread 快照不应被后续 envelope 污染");
  assert.equal(api.bind("test_new_thread_after_second_start", "thread-new-98765"), true, "新 thread 出现在侧边栏后仍应能绑定 draft");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex/thread-selected-12345" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  api.setThread("thread-selected-12345", "fast");
  const turn = api.override({
    type: "mcp-request",
    request: {
      method: "turn/start",
      params: { conversationId: "thread-selected-12345", input: "continue" }
    }
  });
  assert.equal(turn.request.params.serviceTier, "priority", "Fast thread 的 turn/start 应使用 priority");

  api.setThread("thread-selected-12345", "standard");
  const resume = api.override({
    type: "worker-request",
    request: {
      method: "thread/resume",
      params: { threadId: "thread-selected-12345", serviceTier: "priority" }
    }
  });
  assert.equal(resume.request.params.serviceTier, null, "显式 Standard 覆盖应清除 priority");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex/thread-selected-12345" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  api.setThread("thread-selected-12345", "fast");
  const start = api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { prompt: "new conversation" }
  });
  assert.equal(start.params.serviceTier, undefined, "thread/start 不应回退到当前旧 thread 的 Fast 覆盖");

  api.override({ type: "mcp-request", request: null });
  assert.ok(
    fixture.bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "thread_fast_request_override_unsupported"),
    "支持 type 但结构不符合预期时应写诊断"
  );
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  api.setDraft("fast");
  fixture.context.window.location.href = "https://chatgpt.com/codex/thread-selected-12345";
  const state = api.uiState();
  assert.equal(state.source, "none", "已有会话没有 override 时不应显示新对话 draft 状态");
  assert.equal(state.sessionId, "selected-12345", "已有会话 UI 状态应保留当前 thread id");
  assert.equal(state.mode, "standard", "已有会话没有 override 时应显示 Standard");

  const start = api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { threadId: "selected-12345", prompt: "existing thread start" }
  });
  assert.equal(start.params.serviceTier, undefined, "带已有 threadId 的 thread/start 不应消费 draft");
  assert.ok(api.state().draft, "已有 threadId 请求不能清理下一条新对话 draft");
}

{
  const fixture = createFixture({
    url: "https://chatgpt.com/codex",
    enhancementSettings: { scrollRestore: false }
  });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  await fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  const start = api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { prompt: "hello without scroll restore" }
  });
  assert.equal(start.params.serviceTier, "priority", "scrollRestore 关闭时新对话首请求仍应使用 priority");
  fixture.context.window.location.href = "https://chatgpt.com/codex/thread-new-98765";
  const routeInterval = fixture.intervals.find((item) => item.delay === 650);
  assert.ok(routeInterval, "Fast draft 绑定应安装独立 route/session 监听");
  routeInterval.callback();
  await flushAsyncWork();
  assert.equal(api.state().entries["new-98765"].mode, "fast", "scrollRestore 关闭时 draft 仍应绑定到新 thread");
  assert.equal(api.state().draft, null, "绑定成功后应清理 draft");
}

{
  const fixture = createFixture({ delaySettingStorageImport: true });
  await flushAsyncWork();
  const installInterval = fixture.intervals.find((item) => item.delay === 1500);
  assert.ok(installInterval, "应启动刷新 interval");
  installInterval.callback();
  installInterval.callback();
  await flushAsyncWork();
  assert.equal(fixture.settingStorageImportStarted(), 1, "dispatcher patch 加载中重复调用不应重复 import");
  fixture.releaseSettingStorageImport();
  await flushAsyncWork();
  const dispatcher = fixture.dispatcher;
  dispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "hello" }
  });
  assert.equal(fixture.dispatchedMessages.length, 1, "dispatcher 只能被包装一层");
}

console.log("renderer-inject fixture tests passed");
