const { listen } = (window as any).__TAURI__.event;

import type { DownloadProgress, OverallProgress } from "./tauri.js";

export function onDownloadProgress(cb: (p: DownloadProgress) => void): Promise<() => void> {
  return listen("download-progress", (event: any) => cb(event.payload));
}

export function onDownloadStatus(cb: (p: DownloadProgress) => void): Promise<() => void> {
  return listen("download-status", (event: any) => cb(event.payload));
}

export function onOverallProgress(cb: (p: OverallProgress) => void): Promise<() => void> {
  return listen("download-overall", (event: any) => cb(event.payload));
}

export function onCollectionLoading(cb: (p: { loaded: number; total: number }) => void): Promise<() => void> {
  return listen("collection-loading", (event: any) => cb(event.payload));
}
