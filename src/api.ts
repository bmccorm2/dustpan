import { invoke } from "@tauri-apps/api/core";

export interface Failure {
  path: string;
  reason: string;
}

export interface ActionResult {
  done: string[];
  failed: Failure[];
  bytes: number;
}

export type Cat = "appCaches" | "logs" | "browsers" | "developer" | "ai" | "leftovers" | "trash";

export interface CleanItem {
  id: string;
  category: Cat;
  label: string;
  paths: string[];
  size: number;
  recommended: boolean;
  blockedReason: string | null;
}

export interface AppInfo {
  name: string;
  bundleId: string;
  version: string;
  path: string;
  size: number;
  icon: string | null;
}

export interface Leftover {
  path: string;
  size: number;
  kind: string;
}

export type StartupKind = "userAgent" | "globalAgent" | "globalDaemon" | "loginItem";

export interface StartupItem {
  id: string;
  kind: StartupKind;
  label: string;
  path: string;
  program: string | null;
  enabled: boolean;
  broken: boolean;
  editable: boolean;
}

export interface StartupList {
  items: StartupItem[];
  warning: string | null;
}

export interface TaskInfo {
  id: string;
  title: string;
  description: string;
  needsAdmin: boolean;
}

export type TaskStatus = "running" | "done" | "skipped" | "failed";

export interface TaskResult {
  id: string;
  status: TaskStatus;
  message: string;
}

export type EntryKind = "image" | "video" | "audio" | "archive" | "diskImage" | "app" | "folder" | "other";

export interface Entry {
  name: string;
  path: string;
  size: number;
  isDir: boolean;
  kind: EntryKind;
}

export interface DirListing {
  path: string;
  total: number;
  unreadable: number;
  entries: Entry[];
}

export const api = {
  fdaStatus: () => invoke<boolean>("fda_status"),
  openFdaSettings: () => invoke<void>("open_fda_settings"),
  reveal: (path: string) => invoke<void>("reveal_in_finder", { path }),
  trashPaths: (paths: string[]) => invoke<ActionResult>("trash_paths", { paths }),
  cleanScan: () => invoke<CleanItem[]>("clean_scan"),
  emptyTrash: () => invoke<ActionResult>("empty_trash"),
  trashSize: () => invoke<number>("trash_size"),
  appsList: () => invoke<AppInfo[]>("apps_list"),
  appLeftovers: (bundleId: string, name: string) => invoke<Leftover[]>("app_leftovers", { bundleId, name }),
  uninstallApp: (bundleId: string, name: string, appPath: string | null, leftoverPaths: string[]) =>
    invoke<ActionResult>("uninstall_app", { bundleId, name, appPath, leftoverPaths }),
  startupList: () => invoke<StartupList>("startup_list"),
  startupSetEnabled: (id: string, enabled: boolean) => invoke<void>("startup_set_enabled", { id, enabled }),
  startupRemove: (id: string) => invoke<ActionResult>("startup_remove", { id }),
  optimizeTasks: () => invoke<TaskInfo[]>("optimize_tasks"),
  optimizeRun: (ids: string[]) => invoke<TaskResult[]>("optimize_run", { ids }),
  analyzeScan: (root: string) => invoke<void>("analyze_scan", { root }),
  analyzeList: (path: string) => invoke<DirListing>("analyze_list", { path }),
  analyzeTrash: (path: string) => invoke<ActionResult>("analyze_trash", { path }),
};
