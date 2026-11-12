//! Clean scanner: expands the rule table, dedupes overlapping paths, sizes everything.

pub mod rules;

use crate::{apps, fsutil, safety};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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

/// Scans the rule table plus the generic ~/Library/Caches sweep.
pub fn scan(home: &Path, running: &HashSet<String>, progress: impl Fn(&str) + Sync) -> Vec<CleanItem> {
    let installed = apps::installed_bundle_map(home);
    scan_with(home, running, &installed, progress)
}

fn scan_with(
    home: &Path,
    running: &HashSet<String>,
    installed: &HashMap<String, String>,
    progress: impl Fn(&str) + Sync,
) -> Vec<CleanItem> {
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

    size_items(pending, &progress)
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
        scan_with(h, &running, &HashMap::new(), |_| {})
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
    fn sweep_uses_installed_name() {
        let (_t, h) = home();
        file(&h, "Library/Caches/com.foo.bar/x");
        let installed = HashMap::from([("com.foo.bar".to_string(), "Foo".to_string())]);
        let items = scan_with(&h, &HashSet::new(), &installed, |_| {});
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
