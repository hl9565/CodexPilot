import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { mockBackend } from "./dev/mockSnapshots";

const mode = (import.meta as ImportMeta & { env?: { MODE?: string } }).env?.MODE;
export const isUiPreviewMode = mode === "ui-preview";

export function callBackend<T>(command: string, args?: unknown): Promise<T> {
  if (isUiPreviewMode) {
    return mockBackend<T>(command, args);
  }
  return invoke<T>(command, args as InvokeArgs | undefined);
}
