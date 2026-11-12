//! Data left behind by apps that are no longer installed.
//! Detection rules ported from tw93/Mole `is_bundle_orphaned` (GPL-3.0).

use super::{Cat, Pending};
use crate::safety;
use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

const SCAN_DIRS: &[&str] = &[
    "Library/Application Support",
    "Library/Caches",
    "Library/Containers",
    "Library/Preferences",
    "Library/Saved Application State",
    "Library/HTTPStorages",
    "Library/WebKit",
    "Library/Logs",
    "Library/Cookies",
    "Library/LaunchAgents",
];

const STALE_DAYS: u64 = 30;
const SELF_ID: &str = "com.bryan.molesweep";

/// Never treat these ids as orphans (simple subset of Mole's protection list).
const KEEP_PREFIXES: &[&str] = &[
    "com.apple.",
    "org.cups.",
    "com.agilebits.",
    "com.jetbrains.",
    "com.microsoft.",
    "com.sublimetext.",
    "com.sublimehq.",
    "org.pqrs.karabiner",
];
const KEEP_CONTAINS: &[&str] = &["ssh", "gpg", "gnupg", "inputmethod", "com.apple."];

/// `com.foo.bar` from an entry name like `com.foo.bar.plist`.
fn candidate_id(name: &str) -> Option<&str> {
    let id = [".plist", ".savedState", ".binarycookies"]
        .iter()
        .find_map(|s| name.strip_suffix(s))
        .unwrap_or(name);
    is_reverse_dns(id).then_some(id)
}

/// `^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+){2,}$`
fn is_reverse_dns(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

fn is_protected_id(id: &str) -> bool {
    let lower = id.to_lowercase();
    id == SELF_ID
        || KEEP_PREFIXES.iter().any(|p| lower.starts_with(p))
        || KEEP_CONTAINS.iter().any(|p| lower.contains(p))
        || safety::is_sensitive(id)
}

/// True if `id` or a parent id (`com.foo.bar` for `com.foo.bar.helper`) is installed.
fn is_installed(id: &str, installed: &HashSet<String>) -> bool {
    let mut cur = id;
    loop {
        if installed.contains(cur) {
            return true;
        }
        match cur.rfind('.') {
            Some(i) if cur[..i].matches('.').count() >= 2 => cur = &cur[..i],
            _ => return false,
        }
    }
}

/// Bundle ids of `.app` bundles in the usual install locations (up to 3 levels deep).
pub fn installed_bundle_ids(home: &Path) -> HashSet<String> {
    let roots = [
        PathBuf::from("/Applications"),
        home.join("Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices"),
        PathBuf::from("/Library/Application Support"),
        home.join("Library/Application Support"),
    ];
    let mut ids = HashSet::new();
    for r in roots {
        find_apps(&r, 3, &mut ids);
    }
    ids
}

fn find_apps(dir: &Path, depth: u32, ids: &mut HashSet<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "app") {
            if let Some(id) = safety::bundle_id(&p) {
                ids.insert(id);
            }
        } else if depth > 1 && e.file_type().is_ok_and(|t| t.is_dir()) {
            find_apps(&p, depth - 1, ids);
        }
    }
}

/// True if nothing inside `path` changed in the last `days`. Walk errors count as "not stale".
fn is_stale(path: &Path, now: SystemTime, days: u64) -> bool {
    let cutoff = now - Duration::from_secs(days * 86_400);
    let cutoff = cutoff.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let Ok(meta) = std::fs::symlink_metadata(path) else { return false };
    if meta.mtime() >= cutoff {
        return false;
    }
    if !meta.is_dir() {
        return true;
    }
    for entry in jwalk::WalkDir::new(path)
        .follow_links(false)
        .skip_hidden(false)
        .sort(false)
        .parallelism(jwalk::Parallelism::Serial)
    {
        let Ok(entry) = entry else { return false };
        match entry.metadata() {
            Ok(m) if m.mtime() < cutoff => {}
            _ => return false,
        }
    }
    true
}

/// Asks Spotlight whether an app with this bundle id exists anywhere.
/// On timeout or error, says yes (keep the data).
pub fn mdfind_has_app(id: &str) -> bool {
    let child = Command::new("/usr/bin/mdfind")
        .arg(format!("kMDItemCFBundleIdentifier == '{id}'"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return true };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return true;
                }
                let mut out = String::new();
                if let Some(mut s) = child.stdout.take() {
                    use std::io::Read;
                    let _ = s.read_to_string(&mut out);
                }
                return !out.trim().is_empty();
            }
            Ok(None) if start.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return true;
            }
        }
    }
}

/// One pending item per orphaned bundle id.
pub(super) fn scan(
    home: &Path,
    installed: &HashSet<String>,
    has_app: &(dyn Fn(&str) -> bool + Sync),
    now: SystemTime,
) -> Vec<Pending> {
    // Cheap filters first; collect candidates per id.
    let mut by_id: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for d in SCAN_DIRS {
        let Ok(rd) = std::fs::read_dir(home.join(d)) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(id) = candidate_id(&name) else { continue };
            if is_protected_id(id) || is_installed(id, installed) {
                continue;
            }
            let p = e.path();
            if safety::check(&p, home).is_ok() {
                by_id.entry(id.to_string()).or_default().push(p);
            }
        }
    }

    // An id is an orphan only if every entry is stale and Spotlight knows no such app
    // (one mdfind per id, so results are cached by construction).
    use rayon::prelude::*;
    let ids: Vec<(String, Vec<PathBuf>)> = by_id
        .into_iter()
        .filter(|(_, paths)| paths.iter().all(|p| is_stale(p, now, STALE_DAYS)))
        .collect();
    ids.into_par_iter()
        .filter(|(id, _)| !has_app(id))
        .map(|(id, paths)| Pending {
            id: format!("leftover:{id}"),
            category: Cat::Leftovers,
            label: id,
            paths,
            recommended: false,
            blocked_reason: None,
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn backdate(p: &Path, days: u64) {
    let t = SystemTime::now() - Duration::from_secs(days * 86_400);
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as libc::time_t;
    let tv = [libc::timeval { tv_sec: secs, tv_usec: 0 }; 2];
    let c = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: valid NUL-terminated path and a two-element timeval array.
    assert_eq!(unsafe { libc::utimes(c.as_ptr(), tv.as_ptr()) }, 0);
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

    /// Creates `rel` (a dir with one file) and backdates everything in it.
    fn entry(h: &Path, rel: &str, days: u64) -> PathBuf {
        let p = h.join(rel);
        if rel.ends_with(".plist") {
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, b"x").unwrap();
        } else {
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("data"), b"x").unwrap();
            backdate(&p.join("data"), days);
        }
        backdate(&p, days);
        p
    }

    fn ids(items: &[Pending]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    fn run(h: &Path, installed: &[&str]) -> Vec<Pending> {
        let installed = installed.iter().map(|s| s.to_string()).collect();
        scan(h, &installed, &|_| false, SystemTime::now())
    }

    #[test]
    fn detects_orphan() {
        let (_t, h) = home();
        let a = entry(&h, "Library/Application Support/com.gone.app", 60);
        let b = entry(&h, "Library/Preferences/com.gone.app.plist", 60);
        let items = run(&h, &[]);
        assert_eq!(ids(&items), vec!["com.gone.app"]);
        assert_eq!(items[0].id, "leftover:com.gone.app");
        assert!(!items[0].recommended);
        let mut paths = items[0].paths.clone();
        paths.sort();
        assert_eq!(paths, vec![a, b]);
    }

    #[test]
    fn keeps_recent() {
        let (_t, h) = home();
        let p = entry(&h, "Library/Caches/com.gone.app", 60);
        // A file written yesterday keeps the whole folder.
        fs::write(p.join("new"), b"x").unwrap();
        backdate(&p.join("new"), 1);
        backdate(&p, 60);
        assert!(run(&h, &[]).is_empty());
    }

    #[test]
    fn keeps_apple_installed_and_spotlight() {
        let (_t, h) = home();
        entry(&h, "Library/Caches/com.apple.x", 60);
        entry(&h, "Library/Group Containers/group.com.apple.mail", 60);
        entry(&h, "Library/Caches/group.com.apple.notes", 60);
        entry(&h, "Library/Caches/com.here.app", 60);
        entry(&h, "Library/Caches/com.here.app.helper", 60);
        entry(&h, "Library/Caches/com.found.app", 60);
        entry(&h, "Library/Caches/NotReverseDns", 60);
        entry(&h, "Library/Caches/com.bryan.molesweep", 60);
        entry(&h, "Library/Caches/com.1password.x", 60);
        let installed = ["com.here.app".to_string()].into_iter().collect();
        let items = scan(&h, &installed, &|id| id == "com.found.app", SystemTime::now());
        assert!(items.is_empty(), "{:?}", ids(&items));
    }

    #[test]
    fn reverse_dns() {
        assert!(is_reverse_dns("com.foo.bar"));
        assert!(is_reverse_dns("com.foo-x.bar.baz"));
        assert!(!is_reverse_dns("com.foo"));
        assert!(!is_reverse_dns("com..foo"));
        assert!(!is_reverse_dns("com.foo bar.x"));
        assert_eq!(candidate_id("com.a.b.savedState"), Some("com.a.b"));
        assert_eq!(candidate_id("com.a.b.binarycookies"), Some("com.a.b"));
    }
}
