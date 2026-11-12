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
};
