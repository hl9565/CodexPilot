import { resolveAutoLaunchAction, type AutoLaunchDecision } from "./autoLaunch.js";

function expectEqual(actual: AutoLaunchDecision, expected: AutoLaunchDecision, name: string) {
  const actualJson = JSON.stringify(actual);
  const expectedJson = JSON.stringify(expected);
  if (actualJson !== expectedJson) {
    throw new Error(`${name}\nexpected: ${expectedJson}\nactual:   ${actualJson}`);
  }
}

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: false,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
  }),
  { kind: "skip", markAttempted: false },
  "does not auto launch when the preference is off",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
  }),
  {
    kind: "run",
    markAttempted: true,
    command: "launch_codex",
    progress: "正在自动启动 Codex",
    message: "正在自动启动 Codex",
  },
  "launches Codex when auto launch is enabled and state is safe",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "reinject",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
  }),
  {
    kind: "run",
    markAttempted: true,
    command: "reinject_codex",
    progress: "正在自动注入 CodexPilot",
    message: "正在自动注入 CodexPilot",
  },
  "automatically injects into an already running Codex when state is safe",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "restart",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
  }),
  {
    kind: "stop",
    markAttempted: true,
    message: "Codex 已运行但没有调试端口，需要手动确认重启并注入",
  },
  "does not restart an unrelated running Codex",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: true,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
  }),
  { kind: "skip", markAttempted: false },
  "does not run more than once per page load",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: true,
    codexInstalled: true,
  }),
  { kind: "skip", markAttempted: false },
  "does not start a second launch while launch is in progress",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: true,
    launching: false,
    codexInstalled: true,
  }),
  { kind: "skip", markAttempted: false },
  "does not retry automatically after one failed auto action",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: false,
  }),
  {
    kind: "stop",
    markAttempted: true,
    message: "未找到 Codex 安装或启动路径不可用，已跳过自动启动/注入",
  },
  "does not auto launch when Codex is not installed",
);
