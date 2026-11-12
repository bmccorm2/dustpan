//! Clean scanner: expands the rule table, dedupes overlapping paths, sizes everything.

pub mod ai_versions;
pub mod leftovers;
pub mod rules;

use crate::{apps, fsutil, safety};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Cat {
    AppCaches,
    Logs,
    Browsers,
    Developer,
    Ai,
    Leftovers,
    Trash,
}

pub struct Rule {
    pub id: &'static str,
    pub category: Cat,
    pub label: &'static str,
    /// Relative to home.
    pub globs: &'static [&'static str],
    pub recommended: bool,
    pub app_bundle: Option<&'static str>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanItem {
    pub id: String,
    pub category: Cat,
    pub label: String,
    pub paths: Vec<String>,
    pub size: u64,
    pub recommended: bool,
    pub blocked_reason: Option<String>,
}

/// An item before sizing.
struct Pending {
    id: String,
    category: Cat,
    label: String,
    paths: Vec<PathBuf>,
    recommended: bool,
    blocked_reason: Option<String>,
}

/// Tracks which paths earlier rules already own.
#[derive(Default)]
struct Claims {
    claimed: HashSet<PathBuf>,
    /// Every ancestor of a claimed path.
    ancestors: HashSet<PathBuf>,
}

impl Claims {
    fn is_claimed(&self, p: &Path) -> bool {
        p.ancestors().any(|a| self.claimed.contains(a))
    }

    fn claim(&mut self, p: &Path) {
        for a in p.ancestors().skip(1) {
            if !self.ancestors.insert(a.to_path_buf()) {
                break;
            }
        }
        self.claimed.insert(p.to_path_buf());
    }

    /// Claims `p`, or the parts of it not already owned. Returns the new paths.
    fn take(&mut self, p: &Path, home: &Path, out: &mut Vec<PathBuf>) {
        if self.is_claimed(p) || safety::check(p, home).is_err() {
            return;
        }
        if self.ancestors.contains(p) {
            // Contains paths that an earlier rule owns: take only the rest.
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut kids: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
                kids.sort();
                for k in kids {
                    self.take(&k, home, out);
                }
            }
            return;
        }
        self.claim(p);
        out.push(p.to_path_buf());
    }
}

fn expand(home: &Path, rel_glob: &str) -> Vec<PathBuf> {
    let pattern = format!("{}/{}", glob::Pattern::escape(&home.to_string_lossy()), rel_glob);
    let mut v: Vec<PathBuf> = glob::glob(&pattern)
        .map(|it| it.flatten().collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// First word of a label ("Safari cache" -> "Safari").
fn app_name(label: &str) -> &str {
    label.split_whitespace().next().unwrap_or(label)
}

/// What the scanner needs to know about the system (injected so tests can fake it).
struct Env<'a> {
    running: &'a HashSet<String>,
    /// Bundle id -> app name.
    names: &'a HashMap<String, String>,
    /// Every installed bundle id.
    ids: &'a HashSet<String>,
    has_app: &'a (dyn Fn(&str) -> bool + Sync),
}

/// Scans the rule table, AI old versions, the generic ~/Library/Caches sweep,
/// leftovers and the Trash.
pub fn scan(home: &Path, running: &HashSet<String>, progress: impl Fn(&str) + Sync) -> Vec<CleanItem> {
    progress("Finding installed apps");
    let names = apps::installed_bundle_map(home);
    let mut ids = leftovers::installed_bundle_ids(home);
    ids.extend(names.keys().cloned());
    let env = Env { running, names: &names, ids: &ids, has_app: &leftovers::mdfind_has_app };
    scan_with(home, &env, progress)
}

fn scan_with(home: &Path, env: &Env, progress: impl Fn(&str) + Sync) -> Vec<CleanItem> {
    let (running, installed) = (env.running, env.names);
    let mut claims = Claims::default();
    let mut pending = Vec::new();

    for rule in rules::RULES {
        let mut paths = Vec::new();
        for g in rule.globs {
            for m in expand(home, g) {
                claims.take(&m, home, &mut paths);
            }
        }
        if paths.is_empty() {
            continue;
        }
        let blocked_reason = rule
            .app_bundle
            .filter(|b| running.contains(*b))
            .map(|_| format!("Quit {} first", app_name(rule.label)));
        pending.push(Pending {
            id: rule.id.to_string(),
            category: rule.category,
            label: rule.label.to_string(),
            paths,
            recommended: rule.recommended,
            blocked_reason,
        });
    }

    // Generic sweep: one item per child of ~/Library/Caches.
    let caches = home.join("Library/Caches");
    let mut kids: Vec<PathBuf> = std::fs::read_dir(&caches)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    kids.sort();
    for k in kids {
        let name = k.file_name().unwrap_or_default().to_string_lossy().to_string();
        if name.starts_with("com.apple.") || rules::SWEEP_SKIP.contains(&name.as_str()) {
            continue;
        }
        let mut paths = Vec::new();
        claims.take(&k, home, &mut paths);
        if paths.is_empty() {
            continue;
        }
        pending.push(Pending {
            id: format!("appcache:{name}"),
            category: Cat::AppCaches,
            label: installed.get(&name).cloned().unwrap_or_else(|| name.clone()),
            blocked_reason: running.contains(&name).then(|| "In use".to_string()),
            paths,
            recommended: true,
        });
    }

    pending.extend(ai_versions::scan(home));

    progress("Looking for leftovers of removed apps");
    for mut p in leftovers::scan(home, env.ids, env.has_app, SystemTime::now()) {
        // Paths a cache rule already owns stay with that rule.
        p.paths.retain(|x| !claims.is_claimed(x) && !claims.ancestors.contains(x));
        if !p.paths.is_empty() {
            pending.push(p);
        }
    }

    if let Some(t) = trash_item(home) {
        pending.push(t);
    }

    size_items(pending, &progress)
}

/// Children of ~/.Trash.
fn trash_children(home: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(home.join(".Trash"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn trash_item(home: &Path) -> Option<Pending> {
    let paths = trash_children(home);
    (!paths.is_empty()).then(|| Pending {
        id: "trash".into(),
        category: Cat::Trash,
        label: "Trash (permanently deleted)".into(),
        paths,
        recommended: false,
        blocked_reason: None,
    })
}

/// Total allocated size of everything in ~/.Trash.
pub fn trash_size(home: &Path) -> u64 {
    trash_children(home).par_iter().map(|p| fsutil::allocated_size(p)).sum()
}

/// Permanently deletes every child of ~/.Trash. This is the only permanent deletion in the app,
/// and it never touches anything outside ~/.Trash.
pub fn empty_trash(home: &Path) -> fsutil::ActionResult {
    let mut res = fsutil::ActionResult::default();
    let trash = home.join(".Trash");
    for p in trash_children(home) {
        let shown = p.to_string_lossy().to_string();
        if p.parent() != Some(trash.as_path()) {
            res.fail(shown, "Not in the Trash");
            continue;
        }
        let size = fsutil::allocated_size(&p);
        let r = match std::fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(&p),
            Ok(_) => std::fs::remove_file(&p),
            Err(e) => Err(e),
        };
        match r {
            Ok(()) => {
                res.bytes += size;
                res.done.push(shown);
            }
            Err(e) => res.fail(shown, e.to_string()),
        }
    }
    res
}

/// Sizes items in parallel and drops empty paths and items.
fn size_items(pending: Vec<Pending>, progress: &(impl Fn(&str) + Sync)) -> Vec<CleanItem> {
    pending
        .into_par_iter()
        .filter_map(|p| {
            progress(&p.label);
            let sized: Vec<(PathBuf, u64)> = p
                .paths
                .into_par_iter()
                .map(|x| {
                    let s = fsutil::allocated_size(&x);
                    (x, s)
                })
                .filter(|(_, s)| *s > 0)
                .collect();
            if sized.is_empty() {
                return None;
            }
            Some(CleanItem {
                id: p.id,
                category: p.category,
                label: p.label,
                size: sized.iter().map(|(_, s)| s).sum(),
                paths: sized.into_iter().map(|(x, _)| x.to_string_lossy().to_string()).collect(),
                recommended: p.recommended,
                blocked_reason: p.blocked_reason,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn home() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().canonicalize().unwrap();
        (t, h)
    }

    fn file(h: &Path, rel: &str) {
        let p = h.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![7u8; 10_000]).unwrap();
    }

    fn run(h: &Path, running: &[&str]) -> Vec<CleanItem> {
        let running = running.iter().map(|s| s.to_string()).collect();
        let env = Env { running: &running, names: &HashMap::new(), ids: &HashSet::new(), has_app: &|_| false };
        scan_with(h, &env, |_| {})
    }

    fn item<'a>(items: &'a [CleanItem], id: &str) -> Option<&'a CleanItem> {
        items.iter().find(|i| i.id == id)
    }

    #[test]
    fn globs_match() {
        let (_t, h) = home();
        file(&h, ".npm/_cacache/content-v2/x");
        file(&h, "Library/Application Support/Google/Chrome/Default/Cache/Cache_Data/f");
        file(&h, "Library/Application Support/Google/Chrome/Profile 1/GPUCache/g");
        let items = run(&h, &[]);
        let npm = item(&items, "npm").unwrap();
        assert_eq!(npm.paths, vec![h.join(".npm/_cacache/content-v2").to_string_lossy()]);
        assert_eq!(npm.category, Cat::Developer);
        assert_eq!(item(&items, "chrome").unwrap().paths.len(), 2);
    }

    #[test]
    fn dedupe_specific_rule_wins() {
        let (_t, h) = home();
        file(&h, "Library/Caches/Google/Chrome/Default/x");
        file(&h, "Library/Caches/Google/Other/y");
        file(&h, "Library/Logs/DiagnosticReports/a.ips");
        file(&h, "Library/Logs/misc.log");
        let items = run(&h, &[]);
        let chrome = item(&items, "chrome").unwrap();
        assert_eq!(chrome.paths, vec![h.join("Library/Caches/Google/Chrome/Default").to_string_lossy()]);
        // The sweep takes only the unclaimed part of "Google".
        let google = item(&items, "appcache:Google").unwrap();
        assert_eq!(google.paths, vec![h.join("Library/Caches/Google/Other").to_string_lossy()]);
        // User logs don't re-count diagnostic reports.
        let logs = item(&items, "userlogs").unwrap();
        assert_eq!(logs.paths, vec![h.join("Library/Logs/misc.log").to_string_lossy()]);
        let all: Vec<&String> = items.iter().flat_map(|i| &i.paths).collect();
        let uniq: HashSet<&String> = all.iter().copied().collect();
        assert_eq!(all.len(), uniq.len());
    }

    #[test]
    fn running_app_blocked() {
        let (_t, h) = home();
        file(&h, "Library/Caches/com.apple.Safari/x");
        file(&h, "Library/Caches/com.foo.bar/x");
        let items = run(&h, &["com.apple.Safari", "com.foo.bar"]);
        assert_eq!(item(&items, "safari").unwrap().blocked_reason.as_deref(), Some("Quit Safari first"));
        assert_eq!(item(&items, "appcache:com.foo.bar").unwrap().blocked_reason.as_deref(), Some("In use"));
        let items = run(&h, &[]);
        assert!(item(&items, "safari").unwrap().blocked_reason.is_none());
    }

    #[test]
    fn sweep_skips_apple_and_system() {
        let (_t, h) = home();
        file(&h, "Library/Caches/com.apple.Music/x");
        file(&h, "Library/Caches/CloudKit/x");
        file(&h, "Library/Caches/com.foo.bar/x");
        let items = run(&h, &[]);
        assert!(item(&items, "appcache:com.apple.Music").is_none());
        assert!(item(&items, "appcache:CloudKit").is_none());
        assert!(item(&items, "appcache:com.foo.bar").is_some());
    }

    #[test]
    fn zero_byte_matches_dropped() {
        let (_t, h) = home();
        fs::create_dir_all(h.join(".cache/uv")).unwrap();
        fs::write(h.join(".cache/uv/empty"), b"").unwrap();
        let items = run(&h, &[]);
        assert!(item(&items, "uv").is_none());
    }

    #[test]
    fn trash_item_and_empty() {
        let (_t, h) = home();
        file(&h, ".Trash/a/x");
        file(&h, ".Trash/b");
        let items = run(&h, &[]);
        let t = item(&items, "trash").unwrap();
        assert_eq!(t.paths.len(), 2);
        assert!(!t.recommended);
        assert_eq!(trash_size(&h), t.size);
        let r = empty_trash(&h);
        assert_eq!(r.done.len(), 2);
        assert_eq!(r.bytes, t.size);
        assert!(h.join(".Trash").exists());
        assert_eq!(fs::read_dir(h.join(".Trash")).unwrap().count(), 0);
    }

    #[test]
    fn leftover_paths_not_double_counted() {
        let (_t, h) = home();
        file(&h, "Library/Caches/com.gone.app/x");
        file(&h, "Library/Application Support/com.gone.app/y");
        for p in ["Library/Caches/com.gone.app/x", "Library/Caches/com.gone.app", "Library/Application Support/com.gone.app/y", "Library/Application Support/com.gone.app"] {
            leftovers::backdate(&h.join(p), 90);
        }
        let items = run(&h, &[]);
        assert!(item(&items, "appcache:com.gone.app").is_some());
        let l = item(&items, "leftover:com.gone.app").unwrap();
        assert_eq!(l.paths, vec![h.join("Library/Application Support/com.gone.app").to_string_lossy()]);
    }

    #[test]
    fn sweep_uses_installed_name() {
        let (_t, h) = home();
        file(&h, "Library/Caches/com.foo.bar/x");
        let installed = HashMap::from([("com.foo.bar".to_string(), "Foo".to_string())]);
        let env = Env { running: &HashSet::new(), names: &installed, ids: &HashSet::new(), has_app: &|_| false };
        let items = scan_with(&h, &env, |_| {});
        assert_eq!(item(&items, "appcache:com.foo.bar").unwrap().label, "Foo");
    }
}
#[cfg(test)]
mod smoke {
    #[test]
    #[ignore]
    fn real_home() {
        let h = dirs::home_dir().unwrap();
        let t = std::time::Instant::now();
        let items = super::scan(&h, &crate::fsutil::running_bundle_ids(), |_| {});
        for i in &items { println!("{:?} {} {} {} {:?}", i.category, i.id, i.size, i.paths.len(), i.blocked_reason); }
        println!("{} items in {:?}", items.len(), t.elapsed());
    }
}
