//! Path guard. Every deletion in Dustpan goes through `check` first.
//! Never widen these rules to make a caller or test pass.

use std::path::{Component, Path, PathBuf};

const SENSITIVE: &[&str] = &[
    "1password",
    "bitwarden",
    "lastpass",
    "keepass",
    "dashlane",
    "enpass",
    "keychain",
];

/// Roots that may never be deleted themselves (relative to home).
const HOME_ROOTS: &[&str] = &[
    "",
    "Library",
    "Library/Caches",
    "Library/Logs",
    "Library/Application Support",
    "Library/Containers",
    "Library/Group Containers",
    "Library/Preferences",
    "Documents",
    "Desktop",
    "Downloads",
    "Pictures",
    "Movies",
    "Music",
    ".Trash",
];

/// Subtrees under home that are never touched.
const HOME_PROTECTED: &[&str] = &[
    "Library/Keychains",
    "Library/Mobile Documents",
    "Library/CloudStorage",
    ".ssh",
    ".gnupg",
];

/// Allowed system roots (the path must be strictly inside one).
const SYSTEM_ALLOWED: &[&str] = &[
    "/Applications",
    "/Library/LaunchAgents",
    "/Library/LaunchDaemons",
    "/Library/Caches",
];

/// True if the lowercased path mentions a password manager or keychain.
pub fn is_sensitive(s: &str) -> bool {
    let lower = s.to_lowercase();
    SENSITIVE.iter().any(|p| lower.contains(p))
}

/// Returns `Err(reason)` if `path` must not be deleted.
pub fn check(path: &Path, home: &Path) -> Result<(), String> {
    let s = path.to_string_lossy();
    if !path.is_absolute() {
        return Err("Not an absolute path".into());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Path contains '..'".into());
    }
    if s.contains('\n') || s.contains('\r') {
        return Err("Path contains a newline".into());
    }
    if std::fs::symlink_metadata(path).is_err() {
        return Err("Path does not exist".into());
    }
    check_location(path, home)?;

    // Re-check what will really be deleted: the parent with symlinks resolved, plus the
    // final name (the Trash moves a symlink itself, not its target).
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        let resolved = parent
            .canonicalize()
            .map_err(|_| "Cannot resolve parent folder".to_string())?
            .join(name);
        if resolved != path {
            let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
            check_location(&resolved, &home)?;
        }
    }

    if let Some(app) = enclosing_app(path)
        && bundle_id(&app).is_some_and(|id| id.starts_with("com.apple."))
    {
        return Err("Apple system app".into());
    }
    Ok(())
}

/// Location-based rules (no filesystem access).
fn check_location(path: &Path, home: &Path) -> Result<(), String> {
    let s = path.to_string_lossy();

    let in_home = path.starts_with(home) && path != home;
    let in_system = SYSTEM_ALLOWED
        .iter()
        .any(|r| path.starts_with(r) && path != Path::new(r));
    if !in_home && !in_system {
        return Err("Outside allowed folders".into());
    }

    if HOME_ROOTS.iter().any(|r| path == home.join(r)) || path == Path::new("/Applications") {
        return Err("Protected folder".into());
    }
    if HOME_PROTECTED.iter().any(|r| path.starts_with(home.join(r))) {
        return Err("Protected folder".into());
    }
    let lower = s.to_lowercase();
    if path.components().any(|c| c.as_os_str() == "TCC")
        || s.contains("com.apple.TCC")
        || lower.contains("backgroundtaskmanagement")
        || s.contains("com.apple.sharedfilelist")
        || s.contains("/System/")
    {
        return Err("Protected system data".into());
    }
    if is_sensitive(&s) {
        return Err("Password manager or keychain data".into());
    }
    Ok(())
}

/// The outermost `.app` bundle in `path`, if it lives under /Applications or /System/Applications.
fn enclosing_app(path: &Path) -> Option<PathBuf> {
    if !(path.starts_with("/Applications") || path.starts_with("/System/Applications")) {
        return None;
    }
    let mut acc = PathBuf::new();
    for c in path.components() {
        acc.push(c);
        if acc.extension().is_some_and(|e| e == "app") {
            return Some(acc);
        }
    }
    None
}

/// `CFBundleIdentifier` from `<app>/Contents/Info.plist`.
pub fn bundle_id(app: &Path) -> Option<String> {
    let v = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    v.as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn home() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().canonicalize().unwrap().join("Users/me");
        fs::create_dir_all(&h).unwrap();
        (t, h)
    }

    fn mk(p: &Path) -> &Path {
        fs::create_dir_all(p).unwrap();
        p
    }

    #[test]
    fn allows_cache_entry() {
        let (_t, h) = home();
        let p = h.join("Library/Caches/com.foo/x");
        assert_eq!(check(mk(&p), &h), Ok(()));
    }

    #[test]
    fn rejects_relative() {
        let (_t, h) = home();
        assert!(check(Path::new("Library/Caches/x"), &h).is_err());
    }

    #[test]
    fn rejects_parent_dir() {
        let (_t, h) = home();
        mk(&h.join("Library/Caches/a"));
        let p = h.join("Library/Caches/a/../a");
        assert!(check(&p, &h).unwrap_err().contains(".."));
    }

    #[test]
    fn rejects_newline() {
        let (_t, h) = home();
        let p = h.join("Library/Caches/a\nb");
        mk(&p);
        assert!(check(&p, &h).unwrap_err().contains("newline"));
    }

    #[test]
    fn rejects_missing() {
        let (_t, h) = home();
        let p = h.join("Library/Caches/nope");
        assert!(check(&p, &h).unwrap_err().contains("exist"));
    }

    #[test]
    fn rejects_outside_allowed() {
        let (t, h) = home();
        let p = mk(&t.path().canonicalize().unwrap().join("elsewhere/x")).to_path_buf();
        assert!(check(&p, &h).unwrap_err().contains("Outside"));
        // /Library is only allowed for the three listed subfolders.
        assert!(check(Path::new("/Library/Preferences"), &h).is_err());
        assert!(check(Path::new("/usr/bin"), &h).is_err());
    }

    #[test]
    fn rejects_system_roots_themselves() {
        let (_t, h) = home();
        assert!(check(Path::new("/Library/Caches"), &h).is_err());
        assert!(check(Path::new("/Applications"), &h).is_err());
    }

    #[test]
    fn rejects_roots() {
        let (_t, h) = home();
        assert!(check(&h, &h).is_err());
        for r in HOME_ROOTS.iter().filter(|r| !r.is_empty()) {
            let p = h.join(r);
            mk(&p);
            assert!(check(&p, &h).is_err(), "{r} should be protected");
        }
        // Trailing slash is still the root.
        let p = PathBuf::from(format!("{}/", h.join("Library/Caches").display()));
        assert!(check(&p, &h).is_err());
    }

    #[test]
    fn rejects_protected_subtrees() {
        let (_t, h) = home();
        for r in HOME_PROTECTED {
            let p = h.join(r).join("x");
            mk(&p);
            assert!(check(&p, &h).is_err(), "{r} should be protected");
        }
    }

    #[test]
    fn rejects_tcc_and_system_markers() {
        let (_t, h) = home();
        for r in [
            "Library/Application Support/com.apple.TCC/x",
            "Library/Application Support/TCC/x",
            "Library/Application Support/com.apple.backgroundtaskmanagementagent",
            "Library/Application Support/com.apple.sharedfilelist/x",
            "Library/Caches/System/x",
        ] {
            let p = h.join(r);
            mk(&p);
            assert!(check(&p, &h).is_err(), "{r} should be protected");
        }
    }

    #[test]
    fn rejects_sensitive_names() {
        let (_t, h) = home();
        for n in [
            "com.1password.1password",
            "Bitwarden",
            "LastPass",
            "KeePassXC",
            "Dashlane",
            "Enpass",
            "keychain-backup",
        ] {
            let p = h.join("Library/Caches").join(n);
            mk(&p);
            assert!(check(&p, &h).unwrap_err().contains("Password"), "{n}");
        }
    }

    #[test]
    fn rejects_symlinked_parent_escape() {
        let (t, h) = home();
        let outside = mk(&t.path().canonicalize().unwrap().join("outside/x")).to_path_buf();
        fs::create_dir_all(h.join("Library/Caches")).unwrap();
        std::os::unix::fs::symlink(outside.parent().unwrap(), h.join("Library/Caches/link")).unwrap();
        // The path looks like it is in home but resolves elsewhere.
        assert!(check(&h.join("Library/Caches/link/x"), &h).is_err());
        // The symlink itself is fine to trash (the Trash moves the link, not the target).
        assert_eq!(check(&h.join("Library/Caches/link"), &h), Ok(()));
    }

    #[test]
    fn rejects_apple_app_bundle() {
        let (_t, h) = home();
        // Real Apple apps under /Applications (Safari lives there on most Macs).
        for app in ["/Applications/Safari.app", "/System/Applications/Calculator.app"] {
            if Path::new(app).exists() {
                assert!(check(Path::new(app), &h).is_err(), "{app}");
            }
        }
        assert_eq!(enclosing_app(Path::new("/Applications/X.app/Contents/MacOS")), Some(PathBuf::from("/Applications/X.app")));
        assert_eq!(enclosing_app(&h.join("X.app")), None);
    }
}
