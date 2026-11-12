//! Filesystem and macOS helpers: sizes, Trash, FDA check, Finder reveal, admin shell.

use crate::safety;
use serde::Serialize;
use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub path: String,
    pub reason: String,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub done: Vec<String>,
    pub failed: Vec<Failure>,
    pub bytes: u64,
}

impl ActionResult {
    pub fn fail(&mut self, path: impl Into<String>, reason: impl Into<String>) {
        self.failed.push(Failure {
            path: path.into(),
            reason: reason.into(),
        });
    }
}

/// Allocated size on disk (blocks * 512). Does not follow symlinks, counts hard links once,
/// silently skips unreadable entries. Runs serially; callers parallelize across paths.
pub fn allocated_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.blocks() * 512;
    }
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut total = 0;
    for entry in jwalk::WalkDir::new(path)
        .follow_links(false)
        .skip_hidden(false)
        .sort(false)
        .parallelism(jwalk::Parallelism::Serial)
        .into_iter()
        .flatten()
    {
        let Ok(m) = entry.metadata() else { continue };
        if m.nlink() > 1 && !m.is_dir() && !seen.insert((m.dev(), m.ino())) {
            continue;
        }
        total += m.blocks() * 512;
    }
    total
}

fn trash_ctx() -> trash::TrashContext {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut ctx = trash::TrashContext::default();
    ctx.set_delete_method(DeleteMethod::NsFileManager);
    ctx
}

/// Moves each path to the Trash, one at a time, after the safety check.
pub fn trash_paths(paths: &[PathBuf], home: &Path) -> ActionResult {
    let ctx = trash_ctx();
    let mut res = ActionResult::default();
    for p in paths {
        let shown = p.to_string_lossy().to_string();
        if let Err(reason) = safety::check(p, home) {
            res.fail(shown, reason);
            continue;
        }
        let size = allocated_size(p);
        match ctx.delete(p) {
            Ok(()) => {
                res.bytes += size;
                res.done.push(shown);
            }
            Err(e) => res.fail(shown, trash_error(&e)),
        }
    }
    res
}

fn trash_error(e: &trash::Error) -> String {
    let s = e.to_string();
    if s.contains("permission") || s.contains("Operation not permitted") || s.contains("NSCocoaErrorDomain Code=513") {
        format!("Permission denied ({s})")
    } else {
        s
    }
}

/// True if the error text looks like a permission problem.
pub fn is_permission_error(reason: &str) -> bool {
    let l = reason.to_lowercase();
    l.contains("permission") || l.contains("not permitted") || l.contains("code=513")
}

/// Heuristic Full Disk Access check: Safari's data folder is TCC-protected.
pub fn has_full_disk_access(home: &Path) -> bool {
    use std::io::ErrorKind;
    for p in ["Library/Safari", "Library/Containers/com.apple.stocks"] {
        match std::fs::read_dir(home.join(p)) {
            Ok(_) => return true,
            Err(e) if e.kind() == ErrorKind::PermissionDenied => return false,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(_) => return false,
        }
    }
    true
}

/// Bundle ids of running apps, from `lsappinfo list`.
pub fn running_bundle_ids() -> HashSet<String> {
    let out = Command::new("/usr/bin/lsappinfo").arg("list").output();
    match out {
        Ok(o) => parse_bundle_ids(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => HashSet::new(),
    }
}

fn parse_bundle_ids(s: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    for part in s.split("bundleID=\"").skip(1) {
        if let Some(end) = part.find('"')
            && end > 0
        {
            ids.insert(part[..end].to_string());
        }
    }
    ids
}

/// Runs a shell script as root via the macOS password prompt.
pub fn admin_shell(script: &str) -> Result<String, String> {
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let apple = format!("do shell script \"{escaped}\" with administrator privileges");
    let out = Command::new("/usr/bin/osascript")
        .args(["-e", &apple])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if err.contains("-128") {
        Err("Cancelled".into())
    } else {
        Err(err)
    }
}

/// Shows the path in Finder.
pub fn reveal(path: &Path) -> Result<(), String> {
    Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| if s.success() { Ok(()) } else { Err("open -R failed".into()) })
}

/// Current user's uid.
pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn hard_links_counted_once() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("d");
        fs::create_dir(&d).unwrap();
        fs::write(d.join("a"), vec![1u8; 100_000]).unwrap();
        let one = allocated_size(&d);
        fs::hard_link(d.join("a"), d.join("b")).unwrap();
        assert_eq!(allocated_size(&d), one);
        assert!(one >= 100_000);
    }

    #[test]
    fn symlinks_not_followed() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("big"), vec![1u8; 200_000]).unwrap();
        let d = t.path().join("d");
        fs::create_dir(&d).unwrap();
        std::os::unix::fs::symlink(t.path().join("big"), d.join("link")).unwrap();
        assert!(allocated_size(&d) < 200_000);
    }

    #[test]
    fn parses_lsappinfo() {
        let s = r#"1) "Finder" ASN:0x0-0x1001:
    bundleID="com.apple.finder"
2) "Safari" bundleID="com.apple.Safari" pid = 3"#;
        let ids = parse_bundle_ids(s);
        assert!(ids.contains("com.apple.finder") && ids.contains("com.apple.Safari"));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn trash_rejects_unsafe() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().canonicalize().unwrap();
        let r = trash_paths(&[home.join("Library")], &home);
        assert!(r.done.is_empty());
        assert_eq!(r.failed.len(), 1);
    }
}
