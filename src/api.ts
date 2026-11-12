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

export const api = {
  fdaStatus: () => invoke<boolean>("fda_status"),
  openFdaSettings: () => invoke<void>("open_fda_settings"),
  reveal: (path: string) => invoke<void>("reveal_in_finder", { path }),
  trashPaths: (paths: string[]) => invoke<ActionResult>("trash_paths", { paths }),
};
