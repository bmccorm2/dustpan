//! Installed apps, their leftover data, and uninstall.

use crate::{fsutil, safety};
use base64::Engine;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const SELF_ID: &str = "com.bryan.molesweep";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub path: String,
    pub size: u64,
    pub icon: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub path: String,
    pub size: u64,
    pub kind: String,
}

/// Info.plist fields we use.
struct Bundle {
    path: PathBuf,
    id: String,
    name: String,
    version: String,
    icon_file: Option<String>,
}

fn app_roots(home: &Path) -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications"), home.join("Applications")]
}

/// `*.app` directly in each root, plus one level of plain subfolders.
fn find_bundles(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for r in roots {
        let Ok(rd) = std::fs::read_dir(r) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if is_app(&p) {
                out.push(p);
            } else if e.file_type().is_ok_and(|t| t.is_dir())
                && let Ok(sub) = std::fs::read_dir(&p)
            {
                out.extend(sub.flatten().map(|e| e.path()).filter(|p| is_app(p)));
            }
        }
    }
    out
}

fn is_app(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "app")
}

fn read_bundle(path: &Path) -> Option<Bundle> {
    let v = plist::Value::from_file(path.join("Contents/Info.plist")).ok()?;
    let d = v.as_dictionary()?;
    let get = |k: &str| d.get(k).and_then(|v| v.as_string()).map(str::to_string);
    let id = get("CFBundleIdentifier")?;
    if id.starts_with("com.apple.") || id == SELF_ID {
        return None;
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let name = get("CFBundleDisplayName")
        .or_else(|| get("CFBundleName"))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(stem);
    Some(Bundle {
        path: path.to_path_buf(),
        id,
        name,
        version: get("CFBundleShortVersionString").unwrap_or_default(),
        icon_file: get("CFBundleIconFile"),
    })
}

fn bundles(home: &Path) -> Vec<Bundle> {
    find_bundles(&app_roots(home)).iter().filter_map(|p| read_bundle(p)).collect()
}

/// Bundle id -> app name for apps in /Applications and ~/Applications.
pub fn installed_bundle_map(home: &Path) -> HashMap<String, String> {
    bundles(home).into_iter().map(|b| (b.id, b.name)).collect()
}

/// Installed apps with sizes and icons, sorted by name.
pub fn installed_apps(home: &Path, icon_dir: &Path) -> Vec<AppInfo> {
    let _ = std::fs::create_dir_all(icon_dir);
    let mut apps: Vec<AppInfo> = bundles(home)
        .into_par_iter()
        .map(|b| AppInfo {
            size: fsutil::allocated_size(&b.path),
            icon: icon_data_url(&b, icon_dir),
            path: b.path.to_string_lossy().to_string(),
            name: b.name,
            bundle_id: b.id,
            version: b.version,
        })
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// 64px PNG of the app icon as a data URL, cached in `icon_dir`.
fn icon_data_url(b: &Bundle, icon_dir: &Path) -> Option<String> {
    let png = icon_dir.join(format!("{}.png", b.id));
    if !png.exists() {
        let res = b.path.join("Contents/Resources");
        let icns = b
            .icon_file
            .as_ref()
            .map(|f| if f.ends_with(".icns") { res.join(f) } else { res.join(format!("{f}.icns")) })
            .filter(|p| p.exists())
            .or_else(|| Some(res.join("AppIcon.icns")).filter(|p| p.exists()))?;
        let ok = Command::new("/usr/bin/sips")
            .args(["-s", "format", "png", "--resampleHeightWidth", "64", "64"])
            .arg(&icns)
            .arg("--out")
            .arg(&png)
            .output()
            .is_ok_and(|o| o.status.success());
        if !ok {
            return None;
        }
    }
    let bytes = std::fs::read(&png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// How a folder's entries are matched against the app.
#[derive(Clone, Copy)]
enum Match {
    /// Bundle id (exact or `<id>.` prefix) or exact app name.
    IdOrName,
    /// Bundle id only.
    Id,
    /// Entry name contains the bundle id.
    Contains,
}

const LEFTOVER_DIRS: &[(&str, Match)] = &[
    ("Library/Application Support", Match::IdOrName),
    ("Library/Caches", Match::IdOrName),
    ("Library/Containers", Match::Id),
    ("Library/Group Containers", Match::Contains),
    ("Library/Preferences", Match::Id),
    ("Library/Saved Application State", Match::Id),
    ("Library/HTTPStorages", Match::Id),
    ("Library/WebKit", Match::Id),
    ("Library/Logs", Match::IdOrName),
    ("Library/Cookies", Match::Id),
    ("Library/LaunchAgents", Match::Id),
    ("Library/Application Scripts", Match::Id),
];

fn matches(entry: &str, id: &str, name: &str, m: Match) -> bool {
    let e = entry.to_lowercase();
    let id = id.to_lowercase();
    let by_id = e == id || e.starts_with(&format!("{id}."));
    match m {
        Match::Id => by_id,
        Match::IdOrName => by_id || (name.chars().count() >= 4 && e == name.to_lowercase()),
        Match::Contains => e.contains(&id),
    }
}

/// Data an app left in ~/Library.
pub fn app_leftovers(home: &Path, bundle_id: &str, name: &str) -> Vec<Leftover> {
    if bundle_id.trim().len() < 3 {
        return vec![];
    }
    let mut found = Vec::new();
    for &(dir, m) in LEFTOVER_DIRS {
        let Ok(rd) = std::fs::read_dir(home.join(dir)) else { continue };
        let mut kids: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| matches(&e.file_name().to_string_lossy(), bundle_id, name, m))
            .map(|e| e.path())
            .filter(|p| safety::check(p, home).is_ok())
            .collect();
        kids.sort();
        let kind = Path::new(dir).file_name().unwrap().to_string_lossy().to_string();
        found.extend(kids.into_iter().map(|p| (p, kind.clone())));
    }
    found
        .into_par_iter()
        .map(|(p, kind)| Leftover {
            size: fsutil::allocated_size(&p),
            path: p.to_string_lossy().to_string(),
            kind,
        })
        .collect()
}

/// Moves the app (optional) and the chosen leftovers to the Trash.
pub fn uninstall_app(
    home: &Path,
    bundle_id: &str,
    name: &str,
    app_path: Option<&Path>,
    leftovers: &[PathBuf],
    running: &std::collections::HashSet<String>,
) -> fsutil::ActionResult {
    let mut res = fsutil::ActionResult::default();
    if running.contains(bundle_id) {
        let shown = app_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
        res.fail(shown, format!("Quit {name} first"));
        return res;
    }
    if let Some(app) = app_path
        && safety::bundle_id(app).as_deref() != Some(bundle_id)
    {
        res.fail(app.to_string_lossy(), "App bundle id doesn't match");
        return res;
    }
    // Unload login agents before their plists go to the Trash.
    let agents = home.join("Library/LaunchAgents");
    for p in leftovers.iter().filter(|p| p.parent() == Some(agents.as_path())) {
        let _ = Command::new("/bin/launchctl")
            .arg("bootout")
            .arg(format!("gui/{}", fsutil::uid()))
            .arg(p)
            .output();
    }
    let mut paths: Vec<PathBuf> = app_path.map(Path::to_path_buf).into_iter().collect();
    paths.extend(leftovers.iter().cloned());
    let mut r = fsutil::trash_paths(&paths, home);
    if let Some(app) = app_path {
        let app = app.to_string_lossy();
        for f in r.failed.iter_mut().filter(|f| f.path == app) {
            if fsutil::is_permission_error(&f.reason) {
                f.reason = "Needs admin: remove it in Finder".into();
            }
        }
    }
    std::mem::swap(&mut res, &mut r);
    res
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

    fn make_app(dir: &Path, file: &str, id: &str, name: Option<&str>) -> PathBuf {
        let app = dir.join(file);
        fs::create_dir_all(app.join("Contents")).unwrap();
        let mut d = plist::Dictionary::new();
        d.insert("CFBundleIdentifier".into(), id.into());
        d.insert("CFBundleShortVersionString".into(), "1.2".into());
        if let Some(n) = name {
            d.insert("CFBundleName".into(), n.into());
        }
        plist::Value::Dictionary(d).to_file_xml(app.join("Contents/Info.plist")).unwrap();
        app
    }

    #[test]
    fn finds_and_reads_bundles() {
        let (_t, h) = home();
        let apps = h.join("Applications");
        make_app(&apps, "Foo.app", "com.foo.app", Some("Foo Pro"));
        make_app(&apps, "Bar.app", "com.bar.app", None);
        make_app(&apps.join("Utilities"), "Baz.app", "com.baz.app", None);
        make_app(&apps.join("a/b"), "Deep.app", "com.deep.app", None);
        make_app(&apps, "Mail.app", "com.apple.mail", None);
        let found = find_bundles(&[apps]);
        let mut b: Vec<(String, String)> = found.iter().filter_map(|p| read_bundle(p)).map(|b| (b.id, b.name)).collect();
        b.sort();
        assert_eq!(
            b,
            vec![
                ("com.bar.app".into(), "Bar".into()),
                ("com.baz.app".into(), "Baz".into()),
                ("com.foo.app".into(), "Foo Pro".into()),
            ]
        );
    }

    #[test]
    fn finds_leftovers() {
        let (_t, h) = home();
        for rel in [
            "Library/Application Support/com.foo.app",
            "Library/Application Support/Foo Pro",
            "Library/Caches/com.foo.app.helper",
            "Library/Caches/com.foo.application",
            "Library/Containers/com.foo.app",
            "Library/Group Containers/ABCDE.com.foo.app",
            "Library/Saved Application State/com.foo.app.savedState",
            "Library/Logs/foo pro",
        ] {
            fs::create_dir_all(h.join(rel)).unwrap();
        }
        fs::create_dir_all(h.join("Library/Preferences")).unwrap();
        fs::write(h.join("Library/Preferences/com.foo.app.plist"), b"x").unwrap();
        fs::write(h.join("Library/Preferences/com.other.plist"), b"x").unwrap();
        let l = app_leftovers(&h, "com.foo.app", "Foo Pro");
        let rel: Vec<String> = l.iter().map(|x| x.path.strip_prefix(&*h.to_string_lossy()).unwrap().to_string()).collect();
        assert_eq!(
            rel,
            vec![
                "/Library/Application Support/Foo Pro",
                "/Library/Application Support/com.foo.app",
                "/Library/Caches/com.foo.app.helper",
                "/Library/Containers/com.foo.app",
                "/Library/Group Containers/ABCDE.com.foo.app",
                "/Library/Preferences/com.foo.app.plist",
                "/Library/Saved Application State/com.foo.app.savedState",
                "/Library/Logs/foo pro",
            ]
        );
        assert_eq!(l[2].kind, "Caches");
        // Short names are never matched by name.
        fs::create_dir_all(h.join("Library/Caches/Foo")).unwrap();
        assert!(app_leftovers(&h, "com.x.y", "Foo").is_empty());
    }

    #[test]
    fn running_app_not_uninstalled() {
        let (_t, h) = home();
        let app = make_app(&h.join("Applications"), "Foo.app", "com.foo.app", None);
        let running = ["com.foo.app".to_string()].into_iter().collect();
        let r = uninstall_app(&h, "com.foo.app", "Foo", Some(&app), &[], &running);
        assert!(r.done.is_empty());
        assert_eq!(r.failed[0].reason, "Quit Foo first");
        assert!(app.exists());
    }
}

#[cfg(test)]
mod smoke {
    #[test]
    #[ignore]
    fn real_apps() {
        let h = dirs::home_dir().unwrap();
        let t = std::time::Instant::now();
        let apps = super::installed_apps(&h, &std::env::temp_dir().join("molesweep-icons"));
        let with_icon = apps.iter().filter(|a| a.icon.is_some()).count();
        for a in apps.iter().filter(|a| a.icon.is_none()) { println!("no icon: {} {}", a.name, a.path); }
        println!("{} apps, {} with icons, {:?}", apps.len(), with_icon, t.elapsed());
        let t = std::time::Instant::now();
        let l = super::app_leftovers(&h, "com.raycast.macos", "Raycast");
        println!("{l:?} {:?}", t.elapsed());
    }
}
