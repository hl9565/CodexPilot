import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

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
    return false;
  }

  getBoundingClientRect() {
    return { width: 180, height: 32 };
  }

  set innerHTML(value) {
    this._innerHTML = String(value);
    this._textContent = this._innerHTML.replace(/<[^>]*>/g, "");
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

const document = new MiniDocument();
const selected = makeThreadRow("thread-selected-12345", "测试对话", true);
const other = makeThreadRow("thread-other-12345", "其他对话", false);
document.body.append(selected.listItem, other.listItem);

const bridgeCalls = [];
const context = {
  console: { info() {} },
  setTimeout() {},
  Blob: class {},
  URL: {
    createObjectURL() {
      return "blob:codex-pilot-test";
    },
    revokeObjectURL() {}
  },
  document,
  window: {
    location: { href: "https://chatgpt.com/codex" },
    setTimeout() {},
    confirm(message) {
      assert.equal(message, "确认删除“测试对话”？删除前会创建可撤销备份。");
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

const source = fs.readFileSync(new URL("../assets/inject/renderer-inject.js", import.meta.url), "utf8");
vm.runInNewContext(source, context, { filename: "renderer-inject.js" });

const root = document.getElementById("codex-pilot-root");
assert.ok(root, "应创建 CodexPilot 浮动菜单");
assert.match(root.textContent, /助手|后端状态|导出 Markdown/);
assert.doesNotMatch(root.textContent, /当前会话|删除会话|撤销删除/);
assert.equal(bridgeCalls[0]?.path, "/diagnostics/report");
assert.equal(bridgeCalls[0]?.payload?.event, "loaded");

const buttons = root.querySelectorAll("button");
const floatingExportButton = buttons[1];
const message = root.querySelector(".codex-pilot-message");

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
  .find((button) => button.textContent === "删除");
assert.ok(rowDeleteButton, "应在会话行添加删除按钮");
await rowDeleteButton.click();
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

const undoButton = toast.querySelector("button");
assert.ok(undoButton, "Toast 应提供撤销按钮");
await undoButton.click();
const undoCall = bridgeCalls.find((call) => call.path === "/session/undo");
assert.equal(JSON.stringify(undoCall), JSON.stringify({
  path: "/session/undo",
  payload: { undo_token: "undo-token-1" }
}));
assert.equal(toast.textContent, "已撤销删除");

console.log("renderer-inject fixture tests passed");
