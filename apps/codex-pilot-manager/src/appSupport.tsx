import * as React from "react";
import type { BackendStatus, LaunchSnapshot, Theme } from "./types";
import { THEME_STORAGE_KEY } from "./types";

export function canRunLaunchAction(launch: LaunchSnapshot | null): boolean {
  if (!launch) return false;
  return ["launch", "reinject", "restart", "running"].includes(launch.actionKind);
}

export function backendStatusLabel(status: BackendStatus | null): string {
  if (!status) return "未连接";
  if (status.status === "running") return "已连接";
  return status.status || "未连接";
}

export function loadInitialTheme(): Theme {
  if (typeof window === "undefined") return "light";
  return window.localStorage.getItem(THEME_STORAGE_KEY) === "dark" ? "dark" : "light";
}

export function ProgressDialog({ message }: { message: string }) {
  return (
    <div className="progressOverlay" role="status" aria-live="polite">
      <div className="progressDialog">
        <strong>{message}</strong>
        <div className="progressTrack">
          <span />
        </div>
        <p>正在处理，请稍候。</p>
      </div>
    </div>
  );
}
