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
    this.className = "";
    this.disabled = false;
    this.id = "";
    this.title = "";
    this.style = {};
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

  append(...nodes) {
    for (const node of nodes) {
      this.appendChild(node);
    }
  }

  appendChild(node) {
    node.parentElement = this;
    this.children.push(node);
    return node;
  }

  remove() {
    if (!this.parentElement) return;
    const siblings = this.parentElement.children;
    const index = siblings.indexOf(this);
    if (index >= 0) siblings.splice(index, 1);
    this.parentElement = null;
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
    return { width: 180, height: 32 };
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

function createFixture({ includeOther = true } = {}) {
  const document = new MiniDocument();
  const selected = makeThreadRow("thread-selected-12345", "测试对话", true);
  const other = includeOther ? makeThreadRow("thread-other-12345", "其他对话", false) : null;
  const userMessageOne = new MiniElement("article");
  userMessageOne.setAttribute("data-message-author-role", "user");
  userMessageOne.textContent = "请帮我解释这段代码";
  const userMessageTwo = new MiniElement("article");
  userMessageTwo.setAttribute("data-message-author-role", "user");
  userMessageTwo.textContent = "再帮我补一个测试";
  document.body.append(...[selected.listItem, other?.listItem, userMessageOne, userMessageTwo].filter(Boolean));

  const bridgeCalls = [];
  const confirmMessages = [];
  const intervals = [];
  const storage = new Map();
  const context = {
    console: { info() {} },
    setTimeout(callback, delay = 0) {
      if (typeof callback === "function" && Number(delay) < 1000) callback();
      return 1;
    },
    Blob: class {},
    URL: FixtureURL,
    document,
    history: {
      pushState() {},
      replaceState() {}
    },
    window: {
      location: { href: "https://chatgpt.com/codex" },
      innerHeight: 420,
      scrollY: 0,
      setTimeout(callback, delay = 0) {
        if (typeof callback === "function" && Number(delay) < 1000) callback();
        return 1;
      },
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
        }
      },
      confirm(message) {
        confirmMessages.push(message);
        return true;
      },
      __codexPilotBridge(path, payload) {
        bridgeCalls.push({ path, payload });
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
  context.window.window = context.window;
  context.window.document = document;
  context.window.history = context.history;
  vm.runInNewContext(source, context, { filename: "renderer-inject.js" });
  return { bridgeCalls, confirmMessages, context, document, intervals, other, selected, userMessageOne };
}

async function deleteSelected(fixture) {
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
  const { bridgeCalls, confirmMessages, document, intervals, other, selected, userMessageOne } = fixture;
  const root = document.getElementById("codex-pilot-root");
  assert.ok(root, "应创建 CodexPilot 浮动菜单");
  assert.equal(root.dataset.status, "connected");
  assert.match(root.textContent, /Pilot|导出 Markdown/);
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
  const floatingExportButton = buttons[0];
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

  const rowDeleteButton = selected.row.querySelectorAll("button")
    .find((button) => button.getAttribute("aria-label") === "删除会话");
  assert.ok(rowDeleteButton, "应在会话行添加删除按钮");
  assert.equal(rowDeleteButton.title, "删除会话");
  const rowExportButton = selected.row.querySelectorAll("button")
    .find((button) => button.getAttribute("aria-label") === "导出 Markdown");
  assert.ok(rowExportButton, "应在会话行添加导出按钮");
  const rowActionGroup = selected.row.querySelector(".codex-pilot-row-actions");
  assert.ok(rowActionGroup, "应创建独立的会话行操作组");
  assert.equal(rowActionGroup.children.length, 2, "会话行操作组只包含 CodexPilot 自己的按钮");
  const styleText = document.getElementById("codex-pilot-style").textContent;
  assert.match(styleText, /right:\s*42px;/, "会话行操作组应避开 Codex 原生右侧按钮");
  assert.match(styleText, /mask-image:\s*linear-gradient/, "悬停时应遮罩标题，避免文字与操作按钮重叠");

  const timeline = document.getElementById("codex-pilot-timeline");
  assert.ok(timeline, "应为长对话创建时间线");
  const markers = timeline.querySelectorAll(".codex-pilot-timeline-marker");
  assert.equal(markers.length, 2, "应为两个用户问题创建时间线标记");
  await markers[0].click();
  assert.ok(userMessageOne.scrolledIntoView, "点击时间线标记应滚动到对应消息");
  assert.ok(bridgeCalls.some((call) => call.path === "/diagnostics/report" && call.payload?.event === "timeline_jump"));

  await rowDeleteButton.click();
  assert.deepEqual(confirmMessages, [
    "确认删除“测试对话”？删除前会创建可撤销备份。",
    "你正在删除当前打开的会话。删除后会自动切换到其他会话或返回 Codex 首页，确认继续？"
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
  assert.equal(other.row.clicked, true, "删除当前会话后应切换到其他可用会话");
  const toast = document.body.querySelector(".codex-pilot-toast");
  assert.ok(toast, "删除成功后应显示 Toast");
  assert.match(toast.textContent, /已删除本地会话|撤销/);

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
  assert.equal(fixture.context.window.location.href, "https://chatgpt.com/codex", "没有其他会话时应回到 Codex 首页");
}

console.log("renderer-inject fixture tests passed");
