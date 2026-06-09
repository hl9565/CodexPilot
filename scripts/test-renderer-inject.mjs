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
    this.classList = {
      add: (...names) => {
        const classes = new Set(this.className.split(/\s+/).filter(Boolean));
        names.forEach((name) => classes.add(String(name)));
        this.className = Array.from(classes).join(" ");
      },
      remove: (...names) => {
        const removed = new Set(names.map(String));
        this.className = this.className
          .split(/\s+/)
          .filter((name) => name && !removed.has(name))
          .join(" ");
      },
      contains: (name) => this.className.split(/\s+/).includes(String(name))
    };
  }

  setAttribute(name, value) {
    const text = String(value);
    this.attributes.set(name, text);
    if (name === "id") this.id = text;
    if (name === "class") this.className = text;
    if (name === "disabled") this.disabled = true;
  }

  getAttribute(name) {
    return this.attributes.get(name) ?? null;
  }

  removeAttribute(name) {
    this.attributes.delete(name);
    if (name === "id") this._id = "";
    if (name === "class") this._className = "";
    if (name === "disabled") this.disabled = false;
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
  failSettingStorageImportOnce = false,
  enhancementSettings = {},
  windowFastPatchMarker = "",
  existingDocument = null,
  sharedDispatcher = null,
  sharedStorage = null,
  sharedWindowState = {},
  failSettingStorageImportAlways = false,
  settingStorageExportName = "v",
  includeDistractorSettingStorageExport = false,
  exportDispatcherInstanceDirectly = false
} = {}) {
  const document = existingDocument || new MiniDocument();
  const selected = makeThreadRow("thread-selected-12345", "测试对话", true);
  const other = includeOther ? makeThreadRow("thread-other-12345", "其他对话", false) : null;
  const threadMessages = messages || [
    makeMessage({ text: "请帮我解释这段代码", offsetTop: 120 }),
    makeMessage({ text: "再帮我补一个测试", offsetTop: 760 })
  ];
  if (!existingDocument) {
    document.body.append(...[selected.listItem, other?.listItem, ...threadMessages].filter(Boolean));
  }

  const bridgeCalls = [];
  const navigationStateByCall = [];
  const confirmMessages = [];
  const intervals = [];
  const storage = sharedStorage || new Map();
  const mutationObservers = [];
  const navigationClicks = [];
  const timeoutQueue = [];
  const dispatchedMessages = [];
  const dispatcher = sharedDispatcher || {
    dispatchMessage(type, payload) {
      dispatchedMessages.push({ type, payload });
      return { type, payload };
    }
  };
  let releaseSettingStorageImport = null;
  let settingStorageImportStarted = 0;
  class FixtureDispatcher {
    static getInstance() {
      return dispatcher;
    }

    dispatchMessage() {}
  }
  const settingStorageModule = {
    // 真实 vscode-api 同时导出 dispatcher 类（getInstance）与单例实例（自带 dispatchMessage）。
    // exportDispatcherInstanceDirectly 用于覆盖「解析器直查实例」这条生产路径。
    [settingStorageExportName]: exportDispatcherInstanceDirectly ? dispatcher : FixtureDispatcher
  };
  let distractorGetInstanceCalled = false;
  if (includeDistractorSettingStorageExport) {
    settingStorageModule.a = class FixtureSettingsSingleton {
      static getInstance() {
        distractorGetInstanceCalled = true;
        return {};
      }
    };
  }
  class FixtureMutationObserver {
    constructor(callback) {
      this.callback = callback;
      this.connected = false;
      mutationObservers.push(this);
    }

    observe() {
      this.connected = true;
    }

    disconnect() {
      this.connected = false;
    }

    trigger() {
      // 真实 observer 在 disconnect 期间不会派发回调；模型保持一致，以便覆盖
      // “跑处理器前先 disconnect、跑完再 observe” 的自反馈防护逻辑。
      if (!this.connected) return;
      this.callback([], this);
    }
  }
  const windowObject = sharedWindowState;
  const context = {
    console: { info() {} },
    performance: {
      getEntriesByType(type) {
        return type === "resource"
          ? [{ name: "https://chatgpt.com/assets/vscode-api-fixture.js" }]
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
    window: windowObject
  };
  Object.assign(windowObject, {
      __CODEX_PILOT_TEST__: true,
      __codexPilotFastDispatcherPatchInstalled: windowFastPatchMarker || windowObject.__codexPilotFastDispatcherPatchInstalled,
      __CODEX_PILOT_TEST_LOAD_CODEX_APP_MODULE__(namePart) {
        assert.equal(namePart, "vscode-api-", "Fast dispatcher patch 应加载 Codex vscode-api 模块（dispatcher 单例所在 chunk）");
        settingStorageImportStarted += 1;
        if (failSettingStorageImportAlways) {
          return Promise.reject(new Error("setting storage unavailable"));
        }
        if (failSettingStorageImportOnce && settingStorageImportStarted === 1) {
          return Promise.reject(new Error("setting storage not ready"));
        }
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
  });
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
    distractorGetInstanceCalled: () => distractorGetInstanceCalled,
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
  for (let index = 0; index < 16; index += 1) {
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
  const selectedTitle = selected.row.querySelector("[data-thread-title]");
  assert.ok(
    selectedTitle.classList.contains("codex-pilot-row-title-mask-target"),
    "应给实际标题节点标记 CodexPilot 遮罩 class"
  );
  const styleText = document.getElementById("codex-pilot-style").textContent;
  const rowActionsStyle = styleText.match(/\.codex-pilot-row-actions\s*\{[^}]+\}/)?.[0] || "";
  assert.match(rowActionsStyle, /left:\s*8px;/, "会话行操作组应固定在标题左侧");
  assert.doesNotMatch(rowActionsStyle, /right:\s*\d+px;/, "会话行操作组不应同时保留右侧定位");
  assert.match(
    styleText,
    /\.codex-pilot-row-title-mask-target/,
    "标题遮罩 CSS 应使用 CodexPilot 自己标记的标题节点"
  );
  assert.match(
    styleText,
    /mask-image:\s*linear-gradient\(90deg,\s*transparent 0,\s*transparent 42px,\s*#000 58px/,
    "悬停时应遮罩标题左侧，避免文字与操作按钮重叠"
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
  assert.equal(fastToggle.dataset.patchStatus, "ready", "dispatcher patch 安装成功后 Fast 才可用");
  assert.equal(fastToggle.disabled, false, "dispatcher patch ready 后 Fast 按钮应可点击");
  assert.equal(fastToggle.dataset.source, "global", "Pilot 启用时 Fast 应显示为全局生效");
  const openBeforeFastClick = root.dataset.open;
  await fastToggle.click();
  assert.equal(root.dataset.open, openBeforeFastClick, "点击 Fast 不能打开 Pilot 面板");
  assert.notEqual(root.dataset.open, "true", "点击 Fast 后 Pilot 面板不应处于打开状态");
  assert.equal(fastToggle.dataset.mode, "fast", "全局 Fast 应显示 Fast");

  const start = api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { prompt: "hello" }
  });
  assert.equal(start.params.serviceTier, "priority", "全局 Fast 应让新对话首请求使用 priority");
  const store = api.state();
  assert.equal(store.draft, null, "全局 Fast 不应创建或绑定 draft 状态");
  await panelToggle.click();
  assert.equal(root.dataset.open, "true", "Pilot 面板按钮仍应独立工作");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex/thread-selected-12345" });
  await flushAsyncWork();
  const globalStartPayload = {
    method: "thread/start",
    params: { prompt: "new conversation without draft" }
  };
  fixture.dispatcher.dispatchMessage("send-cli-request-for-host", globalStartPayload);
  assert.equal(
    fixture.dispatchedMessages[0].payload.params.serviceTier,
    "priority",
    "全局 Fast 下支持的 thread/start 必须改写为 priority"
  );

  const unknownPayload = { value: 42 };
  fixture.dispatcher.dispatchMessage("unknown-message", unknownPayload);
  assert.equal(
    fixture.dispatchedMessages[1].payload,
    unknownPayload,
    "未知消息必须透传原始 payload 对象"
  );

  fixture.dispatcher.dispatchMessage("unknown-empty", undefined);
  assert.equal(
    fixture.dispatchedMessages[2].payload,
    undefined,
    "undefined payload 未命中改写时不能被重组成空对象"
  );

  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.setDraft("fast");
  fixture.dispatcher.dispatchMessage("start-conversation", undefined);
  assert.equal(
    fixture.dispatchedMessages[3].payload,
    undefined,
    "全局 Fast 也不能凭空把 undefined start-conversation payload 改成对象"
  );
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
  assert.equal(api.bind("test_old_thread", "thread-selected-12345"), false, "全局 Fast 不依赖 draft 绑定旧 thread");
  assert.equal(api.state().draft, null, "全局 Fast 请求后不应保留 draft");

  fixture.context.window.location.href = "https://chatgpt.com/codex/thread-new-98765";
  assert.equal(api.bind("test_new_thread", "thread-new-98765"), false, "全局 Fast 不需要给新 thread 写入 override");
  assert.equal(api.state().entries["thread-new-98765"], undefined, "全局 Fast 不应污染 per-thread override 表");
  assert.equal(api.state().draft, null, "全局 Fast 始终不保留 draft");
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
  assert.equal(api.state().draft, null, "全局 Fast 首个 start 不应创建 pending draft");

  const newThread = makeThreadRow("thread-new-98765", "新对话", false);
  fixture.document.body.append(newThread.listItem);
  const prewarm = api.override({
    type: "thread-prewarm-start",
    request: {
      params: { prompt: "prewarm after row appears" }
    }
  });
  assert.equal(prewarm.request.params.serviceTier, "priority", "全局 Fast 下 prewarm thread/start 仍应 priority");
  assert.equal(api.state().draft, null, "后续 prewarm 也不应创建 pending draft");
  assert.equal(api.bind("test_new_thread_after_second_start", "thread-new-98765"), false, "全局 Fast 不需要等新 thread 出现在侧边栏后绑定");
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
  assert.equal(resume.request.params.serviceTier, "priority", "全局 Fast 应覆盖显式 Standard thread override");

  api.setThread("thread-selected-12345", "fast");
  const snakeCaseFast = api.override({
    type: "send-cli-request-for-host",
    method: "turn/start",
    params: { threadId: "thread-selected-12345", service_tier: null }
  });
  assert.equal(snakeCaseFast.params.serviceTier, "priority", "Fast 覆盖应设置 camelCase serviceTier");
  assert.equal(snakeCaseFast.params.service_tier, "priority", "原请求存在 service_tier 时必须同步设置 snake_case 字段");

  api.setThread("thread-selected-12345", "standard");
  const snakeCaseStandard = api.override({
    type: "send-cli-request-for-host",
    method: "turn/start",
    params: { threadId: "thread-selected-12345", serviceTier: "priority", service_tier: "priority" }
  });
  assert.equal(snakeCaseStandard.params.serviceTier, "priority", "全局 Fast 应覆盖 Standard camelCase priority");
  assert.equal(snakeCaseStandard.params.service_tier, "priority", "全局 Fast 应覆盖 Standard snake_case priority");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;

  api.clear();
  api.setDraft("fast");
  const directStart = api.override({
    type: "start-conversation",
    prompt: "direct start"
  });
  assert.equal(directStart.serviceTier, "priority", "start-conversation 应消费 Fast draft 并设置 priority");

  api.clear();
  api.setDraft("fast");
  const prewarmForHost = api.override({
    type: "prewarm-thread-start-for-host",
    params: { prompt: "prewarm host" }
  });
  assert.equal(prewarmForHost.params.serviceTier, "priority", "prewarm-thread-start-for-host 应覆盖 thread/start params");

  api.clear();
  api.setDraft("fast");
  const startThreadForHost = api.override({
    type: "start-thread-for-host",
    prompt: "start host"
  });
  assert.equal(startThreadForHost.serviceTier, "priority", "start-thread-for-host 应覆盖自身 payload");

  api.clear();
  api.setThread("thread-selected-12345", "fast");
  const startTurnForHost = api.override({
    type: "start-turn-for-host",
    conversationId: "thread-selected-12345",
    params: { input: "continue" }
  });
  assert.equal(startTurnForHost.params.serviceTier, "priority", "start-turn-for-host 应覆盖 turn/start params");
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
  assert.equal(start.params.serviceTier, "priority", "全局 Fast 下没有 threadId 的 thread/start 也应 priority");

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
  assert.equal(state.source, "global", "已有会话应显示全局 Fast 状态");
  assert.equal(state.sessionId, "thread-selected-12345", "已有会话 UI 状态应保留完整当前 thread id");
  assert.equal(state.mode, "fast", "已有会话应显示 Fast");

  const start = api.override({
    type: "send-cli-request-for-host",
    method: "thread/start",
    params: { threadId: "thread-selected-12345", prompt: "existing thread start" }
  });
  assert.equal(start.params.serviceTier, "priority", "全局 Fast 下带已有 threadId 的 thread/start 也应使用 priority");
  assert.ok(api.state().draft, "测试手工写入的 legacy draft 不应被已有 threadId 请求清理");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex/settings" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  const state = api.uiState();
  assert.equal(state.sessionId, "", "普通 Codex 子路由不能被识别成 thread id");
  assert.equal(state.source, "global", "普通 Codex 子路由也应显示全局 Fast 状态");
  await fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  assert.equal(api.state().entries.settings, undefined, "普通路由名不能写入 Fast thread override");
  assert.equal(api.state().draft, null, "全局 Fast 点击不应创建普通子路由 draft");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex/thread/thread-explicit-12345" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  const fastToggle = fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle");
  await fastToggle.click();
  assert.equal(
    api.uiState().sessionId,
    "thread-explicit-12345",
    "明确 thread 路径上下文后的 id 仍应被识别"
  );
  assert.equal(api.state().entries["thread-explicit-12345"], undefined, "全局 Fast 点击不应写入 per-thread override");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex?thread=thread-query-12345" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  const fastToggle = fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle");
  await fastToggle.click();
  assert.equal(
    api.uiState().sessionId,
    "thread-query-12345",
    "显式 query thread 参数仍应被识别"
  );
  assert.equal(api.state().entries["thread-query-12345"], undefined, "全局 Fast 点击不应写入 query thread override");
}

{
  const fixture = createFixture({ url: "https://chatgpt.com/codex/thread-selected-12345" });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  const fastToggle = fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle");
  await fastToggle.click();
  assert.equal(api.uiState().sessionId, "thread-selected-12345", "URL path thread id 应与 sidebar/request key 保持一致");
  assert.equal(api.state().entries["thread-selected-12345"], undefined, "全局 Fast 点击不应写入 URL path thread override");
  const turn = api.override({
    type: "send-cli-request-for-host",
    method: "turn/start",
    params: { threadId: "thread-selected-12345", input: "continue" }
  });
  assert.equal(turn.params.serviceTier, "priority", "URL 上切换 Fast 后同一 thread 的 turn/start 应使用 priority");
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
  assert.equal(api.state().entries["thread-new-98765"], undefined, "scrollRestore 关闭时全局 Fast 不应绑定新 thread override");
  assert.equal(api.state().draft, null, "全局 Fast 不应产生 draft");
}

{
  const fixture = createFixture({ delaySettingStorageImport: true });
  await flushAsyncWork();
  const fastToggle = fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle");
  assert.equal(fastToggle.disabled, false, "dispatcher patch 加载中 Fast 按钮仍应可点击以展示反馈");
  assert.equal(fastToggle.getAttribute("aria-disabled"), "true", "dispatcher patch 加载中 Fast 按钮应标记为不可用");
  await fastToggle.click();
  assert.equal(fastToggle.dataset.mode, "fast", "patch 未 ready 前仍显示全局 Fast 意图");
  assert.equal(fixture.context.window.__CODEX_PILOT_FAST_TEST__.state().draft, null, "patch 未 ready 前不能写入 draft 状态");
  assert.ok(
    fixture.bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "thread_fast_toggle_blocked"),
    "patch 未 ready 前点击应写入 blocked 诊断"
  );
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

{
  const fixture = createFixture({ failSettingStorageImportOnce: true });
  await flushAsyncWork();
  const root = fixture.document.getElementById("codex-pilot-root");
  const fastToggle = root.querySelector(".codex-pilot-fast-toggle");
  assert.equal(fastToggle.disabled, false, "dispatcher patch 首次失败后 Fast 按钮仍应可点击以展示反馈");
  assert.equal(fastToggle.getAttribute("aria-disabled"), "true", "dispatcher patch 首次失败后 Fast 按钮应标记不可用");
  assert.equal(fastToggle.dataset.patchStatus, "unavailable", "首次加载失败应进入 unavailable 状态");
  assert.equal(fixture.settingStorageImportStarted(), 1, "首次安装应尝试加载一次 dispatcher module");

  const installInterval = fixture.intervals.find((item) => item.delay === 1500);
  assert.ok(installInterval, "应启动刷新 interval");
  installInterval.callback();
  await flushAsyncWork();
  assert.equal(fixture.settingStorageImportStarted(), 2, "失败后的下一轮应重新加载 dispatcher module");
  assert.equal(fastToggle.dataset.patchStatus, "ready", "dispatcher patch 重试成功后 Fast 应可用");
  assert.equal(fastToggle.disabled, false, "dispatcher patch 重试成功后按钮应启用");
  assert.equal(fastToggle.getAttribute("aria-disabled"), null, "dispatcher patch 重试成功后应移除 aria-disabled");

  await fastToggle.click();
  fixture.dispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "hello after retry" }
  });
  assert.equal(
    fixture.dispatchedMessages[0].payload.params.serviceTier,
    "priority",
    "dispatcher patch 重试成功后 Fast draft 应改写首请求"
  );
}

{
  const fixture = createFixture({ windowFastPatchMarker: "3" });
  await flushAsyncWork();
  const root = fixture.document.getElementById("codex-pilot-root");
  const fastToggle = root.querySelector(".codex-pilot-fast-toggle");
  assert.equal(fixture.settingStorageImportStarted(), 1, "window patch 标记不能跳过真实 dispatcher 检查");
  assert.equal(fastToggle.dataset.patchStatus, "ready", "真实 dispatcher 安装后才能显示 ready");

  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  await fastToggle.click();
  fixture.dispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "hello after stale marker" }
  });
  assert.equal(
    fixture.dispatchedMessages[0].payload.params.serviceTier,
    "priority",
    "window 标记存在但实例未安装时仍应安装当前 dispatcher wrapper"
  );
}

{
  const fixture = createFixture({
    url: "https://chatgpt.com/codex",
    settingStorageExportName: "Qx",
    includeDistractorSettingStorageExport: true
  });
  await flushAsyncWork();
  const fastToggle = fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle");
  assert.equal(fastToggle.dataset.patchStatus, "ready", "dispatcher resolver 不应依赖固定混淆导出名");
  assert.equal(
    fixture.distractorGetInstanceCalled(),
    false,
    "dispatcher resolver 不能调用看起来不像 dispatcher 的 getInstance 导出"
  );
  fixture.dispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "alternate export" }
  });
  assert.equal(
    fixture.dispatchedMessages[0].payload.params.serviceTier,
    "priority",
    "非 v 导出的 dispatcher 仍应被 patch 并应用全局 Fast"
  );
}

{
  // 生产路径：Codex 26.x 的 vscode-api 把 dispatcher 以单例实例（自带 dispatchMessage）直接导出，
  // 解析器第一轮直查即命中，且不应触碰任何 getInstance 工厂（避免实例化无关单例的副作用）。
  const fixture = createFixture({
    url: "https://chatgpt.com/codex",
    settingStorageExportName: "f",
    exportDispatcherInstanceDirectly: true,
    includeDistractorSettingStorageExport: true
  });
  await flushAsyncWork();
  const fastToggle = fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle");
  assert.equal(fastToggle.dataset.patchStatus, "ready", "dispatcher 以单例实例直接导出时也应解析成功");
  assert.equal(
    fixture.distractorGetInstanceCalled(),
    false,
    "直查命中实例后不应再调用任何 getInstance"
  );
  fixture.dispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "instance export" }
  });
  assert.equal(
    fixture.dispatchedMessages[0].payload.params.serviceTier,
    "priority",
    "实例导出的 dispatcher 同样应被 patch 并应用全局 Fast"
  );
}

{
  const staleDocument = new MiniDocument();
  const staleStyle = staleDocument.createElement("style");
  staleStyle.id = "codex-pilot-style";
  staleStyle.textContent = "[data-codex-pilot-row='true']:hover [data-thread-title] { opacity: 0.5; }";
  staleDocument.head.appendChild(staleStyle);
  const fixture = createFixture({ existingDocument: staleDocument });
  await flushAsyncWork();
  const styleText = fixture.document.getElementById("codex-pilot-style").textContent;
  assert.match(styleText, /\.codex-pilot-row-title-mask-target/, "重复注入时应覆盖旧 style 内容");
  assert.doesNotMatch(styleText, /\[data-thread-title\]\s*\{ opacity/, "旧 style 内容不能残留为生效样式");
}

{
  const sharedDispatcherMessages = [];
  const sharedDispatcher = {
    dispatchMessage(type, payload) {
      sharedDispatcherMessages.push({ type, payload });
      return { type, payload };
    }
  };
  const sharedStorage = new Map();
  const sharedWindowState = {};
  const enabledFixture = createFixture({
    url: "https://chatgpt.com/codex",
    sharedDispatcher,
    sharedStorage,
    sharedWindowState
  });
  await flushAsyncWork();
  const api = enabledFixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  await enabledFixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  sharedDispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "enabled fast" }
  });
  assert.equal(sharedDispatcherMessages[0].payload.params.serviceTier, "priority", "启用时 Fast wrapper 应改写请求");

  createFixture({
    existingDocument: enabledFixture.document,
    enhancementSettings: { enabled: false },
    sharedDispatcher,
    sharedStorage,
    sharedWindowState,
    failSettingStorageImportAlways: true
  });
  await flushAsyncWork();
  sharedDispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "disabled fast" }
  });
  assert.equal(
    sharedDispatcherMessages[1].payload.params.serviceTier,
    undefined,
    "增强关闭后必须通过已登记 dispatcher 恢复原始 dispatch，不能依赖重新 import 模块"
  );
}

{
  const sharedDispatcherMessages = [];
  const sharedDispatcher = {
    dispatchMessage(type, payload) {
      sharedDispatcherMessages.push({ type, payload });
      return { type, payload };
    }
  };
  const sharedStorage = new Map();
  const sharedWindowState = {};
  const disabledFixture = createFixture({
    url: "https://chatgpt.com/codex",
    delaySettingStorageImport: true,
    enhancementSettings: { enabled: false },
    sharedDispatcher,
    sharedStorage,
    sharedWindowState
  });
  await flushAsyncWork();
  assert.equal(disabledFixture.settingStorageImportStarted(), 1, "增强关闭时应尝试卸载 dispatcher patch");

  const activeFixture = createFixture({
    url: "https://chatgpt.com/codex",
    sharedDispatcher,
    sharedStorage,
    sharedWindowState
  });
  await flushAsyncWork();
  const activeApi = activeFixture.context.window.__CODEX_PILOT_FAST_TEST__;
  activeApi.clear();
  await activeFixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  sharedDispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "active boot before stale disabled release" }
  });
  assert.equal(
    sharedDispatcherMessages[0].payload.params.serviceTier,
    "priority",
    "新 boot 安装成功后应能改写 Fast 请求"
  );
  const activeBootId = sharedWindowState.__CODEX_PILOT_BOOT_ID__;

  disabledFixture.releaseSettingStorageImport();
  await flushAsyncWork();
  activeApi.clear();
  await activeFixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  sharedDispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "active boot after stale disabled release" }
  });
  assert.equal(
    sharedDispatcherMessages[1].payload.params.serviceTier,
    "priority",
    "旧 disabled boot 的延迟卸载完成后不能恢复掉新 boot wrapper"
  );
  assert.equal(
    sharedDispatcher.__codexPilotFastPatchOwnerBootId,
    activeBootId,
    "旧 disabled boot 不能夺走或清空新 boot 的 dispatcher patch owner"
  );
}

{
  const sharedDispatcherMessages = [];
  const sharedDispatcher = {
    dispatchMessage(type, payload) {
      sharedDispatcherMessages.push({ type, payload });
      return { type, payload };
    }
  };
  const sharedStorage = new Map();
  const sharedWindowState = {};
  const staleFixture = createFixture({
    url: "https://chatgpt.com/codex",
    delaySettingStorageImport: true,
    sharedDispatcher,
    sharedStorage,
    sharedWindowState
  });
  await flushAsyncWork();

  const activeFixture = createFixture({
    url: "https://chatgpt.com/codex",
    sharedDispatcher,
    sharedStorage,
    sharedWindowState
  });
  await flushAsyncWork();
  const activeApi = activeFixture.context.window.__CODEX_PILOT_FAST_TEST__;
  activeApi.clear();
  await activeFixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  sharedDispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "active boot before stale release" }
  });
  assert.equal(
    sharedDispatcherMessages[0].payload.params.serviceTier,
    "priority",
    "新 boot 安装成功后应能改写 Fast 请求"
  );

  staleFixture.releaseSettingStorageImport();
  await flushAsyncWork();
  activeApi.clear();
  await activeFixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  sharedDispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "active boot after stale release" }
  });
  assert.equal(
    sharedDispatcherMessages[1].payload.params.serviceTier,
    "priority",
    "旧 boot 的延迟 dispatcher 安装完成后不能覆盖新 boot wrapper"
  );
  assert.equal(
    sharedDispatcher.__codexPilotFastPatchOwnerBootId,
    sharedWindowState.__CODEX_PILOT_BOOT_ID__,
    "dispatcher patch owner 必须保持为当前 active boot"
  );
}

{
  const sharedDispatcherMessages = [];
  const sharedDispatcher = {
    dispatchMessage(type, payload) {
      sharedDispatcherMessages.push({ type, payload });
      return { type, payload };
    },
    __codexPilotOriginalDispatchMessage() {
      throw new Error("stale original dispatch must be ignored");
    },
    __codexPilotFastPatchVersion: "3",
    __codexPilotFastPatchOwnerBootId: "stale-boot"
  };
  const fixture = createFixture({
    url: "https://chatgpt.com/codex",
    sharedDispatcher
  });
  await flushAsyncWork();
  const api = fixture.context.window.__CODEX_PILOT_FAST_TEST__;
  api.clear();
  await fixture.document.getElementById("codex-pilot-root").querySelector(".codex-pilot-fast-toggle").click();
  sharedDispatcher.dispatchMessage("send-cli-request-for-host", {
    method: "thread/start",
    params: { prompt: "stale metadata" }
  });
  assert.equal(
    sharedDispatcherMessages[0].payload.params.serviceTier,
    "priority",
    "dispatcher 上的陈旧 patch metadata 不能让新安装链到旧 wrapper"
  );
}

console.log("renderer-inject fixture tests passed");
