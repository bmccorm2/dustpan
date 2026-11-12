//! Optimize tasks. Commands ported from tw93/Mole `lib/optimize/tasks.sh` (GPL-3.0).

use crate::{clean::leftovers, fsutil, startup};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub needs_admin: bool,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Running,
    Done,
    Skipped,
    Failed,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub id: String,
    pub status: Status,
    pub message: String,
}

pub const TASKS: &[TaskInfo] = &[
    TaskInfo {
        id: "flush_dns",
        title: "Flush DNS cache",
        description: "Clears cached DNS lookups. Helps when sites don't load after a network or DNS change.",
        needs_admin: true,
    },
    TaskInfo {
        id: "rebuild_quicklook",
        title: "Rebuild QuickLook cache",
        description: "Resets QuickLook thumbnails and reloads its plugins. Fixes wrong or missing previews.",
        needs_admin: false,
    },
    TaskInfo {
        id: "saved_state",
        title: "Clear old saved window states",
        description: "Moves window states of apps not used for 30 days to the Trash.",
        needs_admin: false,
    },
    TaskInfo {
        id: "broken_agents",
        title: "Remove broken background services",
        description: "Moves your launch agents whose program no longer exists to the Trash.",
        needs_admin: false,
    },
    TaskInfo {
        id: "broken_login_items",
        title: "Remove broken login items",
        description: "Removes login items that point to apps that no longer exist.",
        needs_admin: false,
    },
];

const STATE_DAYS: u64 = 30;

pub fn run_task(home: &Path, id: &str) -> TaskResult {
    let (status, message) = match id {
        "flush_dns" => match fsutil::admin_shell("dscacheutil -flushcache; killall -HUP mDNSResponder") {
            Ok(_) => (Status::Done, "DNS cache flushed".to_string()),
            Err(e) => (Status::Failed, e),
        },
        "rebuild_quicklook" => rebuild_quicklook(),
        "saved_state" => saved_state(home),
        "broken_agents" => broken_agents(home),
        "broken_login_items" => broken_login_items(home),
        _ => (Status::Failed, "Unknown task".to_string()),
    };
    TaskResult { id: id.to_string(), status, message }
}

fn rebuild_quicklook() -> (Status, String) {
    for args in [&["-r", "cache"][..], &["-r"][..]] {
        match Command::new("/usr/bin/qlmanage").args(args).output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => return (Status::Failed, String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => return (Status::Failed, e.to_string()),
        }
    }
    (Status::Done, "QuickLook cache rebuilt".into())
}

/// Saved window states with nothing changed in the last 30 days.
fn old_saved_states(home: &Path, now: SystemTime) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(home.join("Library/Saved Application State"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    v.retain(|p| leftovers::is_stale(p, now, STATE_DAYS));
    v.sort();
    v
}

fn saved_state(home: &Path) -> (Status, String) {
    let old = old_saved_states(home, SystemTime::now());
    if old.is_empty() {
        return (Status::Skipped, "Nothing older than 30 days".into());
    }
    let r = fsutil::trash_paths(&old, home);
    summarize(r.done.len(), r.failed.len(), format!("Removed {}", r.done.len()))
}

fn summarize(done: usize, failed: usize, msg: String) -> (Status, String) {
    match (done, failed) {
        (_, 0) => (Status::Done, msg),
        (0, f) => (Status::Failed, format!("{f} failed")),
        (_, f) => (Status::Done, format!("{msg}, {f} failed")),
    }
}

fn broken_agents(home: &Path) -> (Status, String) {
    let broken: Vec<startup::StartupItem> = startup::user_agents(home).into_iter().filter(|i| i.broken).collect();
    if broken.is_empty() {
        return (Status::Skipped, "No broken background services".into());
    }
    remove_all(home, &broken)
}

fn broken_login_items(home: &Path) -> (Status, String) {
    let items = match startup::login_items() {
        Ok(v) => v,
        Err(e) => return (Status::Failed, e),
    };
    let broken: Vec<startup::StartupItem> = items.into_iter().filter(|i| i.broken).collect();
    if broken.is_empty() {
        return (Status::Skipped, "No broken login items".into());
    }
    remove_all(home, &broken)
}

fn remove_all(home: &Path, items: &[startup::StartupItem]) -> (Status, String) {
    let (mut ok, mut failed) = (Vec::new(), 0);
    for i in items {
        match startup::remove(home, &i.id) {
            Ok(r) if r.failed.is_empty() => ok.push(i.label.clone()),
            _ => failed += 1,
        }
    }
    summarize(ok.len(), failed, format!("Removed {}", ok.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::leftovers::backdate;
    use std::fs;

    #[test]
    fn saved_state_age_filter() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().canonicalize().unwrap();
        let dir = h.join("Library/Saved Application State");
        for (name, days) in [("com.old.app.savedState", 45), ("com.new.app.savedState", 3)] {
            let p = dir.join(name);
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("windows.plist"), b"x").unwrap();
            backdate(&p.join("windows.plist"), days);
            backdate(&p, days);
        }
        assert_eq!(old_saved_states(&h, SystemTime::now()), vec![dir.join("com.old.app.savedState")]);
    }

    #[test]
    fn saved_state_none() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().canonicalize().unwrap();
        assert_eq!(saved_state(&h).0, Status::Skipped);
    }
}
