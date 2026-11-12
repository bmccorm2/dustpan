//! Launch agents/daemons and login items.

use crate::fsutil::{self, uid};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    UserAgent,
    GlobalAgent,
    GlobalDaemon,
    LoginItem,
}

impl Kind {
    fn tag(self) -> &'static str {
        match self {
            Kind::UserAgent => "userAgent",
            Kind::GlobalAgent => "globalAgent",
            Kind::GlobalDaemon => "globalDaemon",
            Kind::LoginItem => "loginItem",
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub id: String,
    pub kind: Kind,
    pub label: String,
    pub path: String,
    pub program: Option<String>,
    pub enabled: bool,
    pub broken: bool,
    pub editable: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartupList {
    pub items: Vec<StartupItem>,
    pub warning: Option<String>,
}

const LOGIN_WARNING: &str = "Login items can't be read. Allow MoleSweep to control System Events in System Settings › Privacy & Security › Automation.";

/// Labels listed as disabled by `launchctl print-disabled`.
/// Accepts both `"label" => disabled` and `"label" => true`.
pub fn parse_disabled(out: &str) -> HashSet<String> {
    out.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once("=>")?;
            let v = v.trim().trim_end_matches(',');
            matches!(v, "disabled" | "true").then(|| k.trim().trim_matches('"').to_string())
        })
        .collect()
}

fn disabled_in(domain: &str) -> HashSet<String> {
    Command::new("/bin/launchctl")
        .args(["print-disabled", domain])
        .output()
        .map(|o| parse_disabled(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// `(label, program)` from a launchd plist.
fn read_plist(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(v) = plist::Value::from_file(path) else { return (None, None) };
    let Some(d) = v.as_dictionary() else { return (None, None) };
    let label = d.get("Label").and_then(|v| v.as_string()).map(str::to_string);
    let program = d
        .get("Program")
        .and_then(|v| v.as_string())
        .or_else(|| d.get("ProgramArguments")?.as_array()?.first()?.as_string())
        .map(str::to_string);
    (label, program)
}

fn label_of(path: &Path) -> String {
    read_plist(path)
        .0
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string())
}

fn read_launch_dir(dir: &Path, kind: Kind, disabled: &HashSet<String>) -> Vec<StartupItem> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut items: Vec<StartupItem> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "plist"))
        .map(|p| {
            let (label, program) = read_plist(&p);
            let label = label.unwrap_or_else(|| p.file_stem().unwrap_or_default().to_string_lossy().to_string());
            let broken = program
                .as_deref()
                .is_some_and(|pr| pr.starts_with('/') && std::fs::symlink_metadata(pr).is_err());
            let path = p.to_string_lossy().to_string();
            StartupItem {
                id: format!("{}:{path}", kind.tag()),
                kind,
                enabled: !disabled.contains(&label),
                label,
                path,
                program,
                broken,
                editable: kind == Kind::UserAgent,
            }
        })
        .collect();
    items.sort_by_key(|i| i.label.to_lowercase());
    items
}

/// Parses `{name, path} of every login item`, printed as "n1, n2, p1, p2".
fn parse_login_items(out: &str) -> Option<Vec<(String, String)>> {
    let out = out.trim();
    if out.is_empty() {
        return Some(vec![]);
    }
    let parts: Vec<&str> = out.split(", ").collect();
    if parts.len() % 2 != 0 {
        return None;
    }
    let (names, paths) = parts.split_at(parts.len() / 2);
    // A login item without a file reports its path as "missing value".
    let path = |p: &str| if p == "missing value" { String::new() } else { p.to_string() };
    Some(names.iter().zip(paths).map(|(n, p)| (n.to_string(), path(p))).collect())
}

pub fn login_items() -> Result<Vec<StartupItem>, String> {
    let out = Command::new("/usr/bin/osascript")
        .args(["-e", "tell application \"System Events\" to get {name, path} of every login item"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(LOGIN_WARNING.into());
    }
    let pairs = parse_login_items(&String::from_utf8_lossy(&out.stdout)).ok_or("Couldn't parse login items")?;
    Ok(pairs
        .into_iter()
        .map(|(name, path)| StartupItem {
            id: format!("loginItem:{name}"),
            kind: Kind::LoginItem,
            broken: !path.is_empty() && std::fs::symlink_metadata(&path).is_err(),
            label: name,
            path,
            program: None,
            enabled: true,
            editable: true,
        })
        .collect())
}

/// Only ~/Library/LaunchAgents (no launchctl or System Events calls).
pub fn user_agents(home: &Path) -> Vec<StartupItem> {
    read_launch_dir(&home.join("Library/LaunchAgents"), Kind::UserAgent, &HashSet::new())
}

pub fn list(home: &Path) -> StartupList {
    let gui = disabled_in(&format!("gui/{}", uid()));
    let system = disabled_in("system");
    let (mut items, warning) = match login_items() {
        Ok(v) => (v, None),
        Err(w) => (vec![], Some(w)),
    };
    items.extend(read_launch_dir(&home.join("Library/LaunchAgents"), Kind::UserAgent, &gui));
    items.extend(read_launch_dir(Path::new("/Library/LaunchAgents"), Kind::GlobalAgent, &gui));
    items.extend(read_launch_dir(Path::new("/Library/LaunchDaemons"), Kind::GlobalDaemon, &system));
    StartupList { items, warning }
}

/// Splits an id into its kind and path/name, and checks user agents live in ~/Library/LaunchAgents.
fn parse_id(home: &Path, id: &str) -> Result<(Kind, String), String> {
    let (tag, rest) = id.split_once(':').ok_or("Bad id")?;
    let kind = [Kind::UserAgent, Kind::GlobalAgent, Kind::GlobalDaemon, Kind::LoginItem]
        .into_iter()
        .find(|k| k.tag() == tag)
        .ok_or("Bad id")?;
    if kind == Kind::UserAgent && Path::new(rest).parent() != Some(home.join("Library/LaunchAgents").as_path()) {
        return Err("Not in ~/Library/LaunchAgents".into());
    }
    match kind {
        Kind::UserAgent | Kind::LoginItem => Ok((kind, rest.to_string())),
        _ => Err("System items are read-only".into()),
    }
}

fn launchctl(args: &[&str]) -> Result<(), String> {
    let o = Command::new("/bin/launchctl").args(args).output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
        Err(if msg.is_empty() { format!("launchctl {} failed", args[0]) } else { msg })
    }
}

fn disable_agent(path: &Path) -> Result<(), String> {
    let target = format!("gui/{}/{}", uid(), label_of(path));
    launchctl(&["disable", &target])?;
    let _ = launchctl(&["bootout", &target]);
    Ok(())
}

pub fn set_enabled(home: &Path, id: &str, enabled: bool) -> Result<(), String> {
    let (kind, path) = parse_id(home, id)?;
    if kind != Kind::UserAgent {
        return Err("Only your own background services can be toggled".into());
    }
    let path = PathBuf::from(path);
    if !enabled {
        return disable_agent(&path);
    }
    let domain = format!("gui/{}", uid());
    launchctl(&["enable", &format!("{domain}/{}", label_of(&path))])?;
    if let Err(e) = launchctl(&["bootstrap", &domain, &path.to_string_lossy()]) {
        // Already loaded is fine (launchctl reports EIO "5" or EEXIST "17").
        let l = e.to_lowercase();
        if !(l.contains("already") || l.contains(": 5:") || l.contains(": 17:")) {
            return Err(e);
        }
    }
    Ok(())
}

pub fn remove(home: &Path, id: &str) -> Result<fsutil::ActionResult, String> {
    let (kind, rest) = parse_id(home, id)?;
    match kind {
        Kind::UserAgent => {
            let path = PathBuf::from(&rest);
            let _ = disable_agent(&path);
            Ok(fsutil::trash_paths(&[path], home))
        }
        Kind::LoginItem => {
            let name = rest.replace('\\', "\\\\").replace('"', "\\\"");
            let script = format!("tell application \"System Events\" to delete login item \"{name}\"");
            let o = Command::new("/usr/bin/osascript")
                .args(["-e", &script])
                .output()
                .map_err(|e| e.to_string())?;
            let mut r = fsutil::ActionResult::default();
            if o.status.success() {
                r.done.push(rest);
            } else {
                r.fail(rest, String::from_utf8_lossy(&o.stderr).trim().to_string());
            }
            Ok(r)
        }
        _ => Err("System items are read-only".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_print_disabled() {
        let new = "disabled services = {\n\t\"com.a\" => disabled\n\t\"com.b\" => enabled\n}\n";
        let old = "disabled services = {\n\t\"com.c\" => true\n\t\"com.d\" => false\n}\n";
        assert_eq!(parse_disabled(new), HashSet::from(["com.a".to_string()]));
        assert_eq!(parse_disabled(old), HashSet::from(["com.c".to_string()]));
    }

    fn agent(dir: &Path, file: &str, label: Option<&str>, program: Option<&str>, args0: Option<&str>) {
        let mut d = plist::Dictionary::new();
        if let Some(l) = label {
            d.insert("Label".into(), l.into());
        }
        if let Some(p) = program {
            d.insert("Program".into(), p.into());
        }
        if let Some(a) = args0 {
            d.insert("ProgramArguments".into(), plist::Value::Array(vec![a.into(), "--flag".into()]));
        }
        plist::Value::Dictionary(d).to_file_xml(dir.join(file)).unwrap();
    }

    #[test]
    fn reads_agents_and_detects_broken() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path();
        agent(dir, "a.plist", Some("com.ok"), Some("/bin/ls"), None);
        agent(dir, "b.plist", Some("com.broken"), None, Some("/nonexistent/tool"));
        agent(dir, "com.nolabel.plist", None, None, Some("relative-tool"));
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        let disabled = HashSet::from(["com.ok".to_string()]);
        let items = read_launch_dir(dir, Kind::UserAgent, &disabled);
        let by = |l: &str| items.iter().find(|i| i.label == l).unwrap();
        assert_eq!(items.len(), 3);
        assert!(!by("com.ok").broken && !by("com.ok").enabled);
        assert_eq!(by("com.ok").program.as_deref(), Some("/bin/ls"));
        assert!(by("com.broken").broken && by("com.broken").enabled);
        assert_eq!(by("com.broken").program.as_deref(), Some("/nonexistent/tool"));
        // No Label: file stem; relative program is never "broken".
        assert!(!by("com.nolabel").broken);
        assert!(by("com.ok").id.starts_with("userAgent:/"));
    }

    #[test]
    fn parses_login_items() {
        assert_eq!(
            parse_login_items("Raycast, Ghostty, /Applications/Raycast.app, /Applications/Ghostty.app\n"),
            Some(vec![
                ("Raycast".into(), "/Applications/Raycast.app".into()),
                ("Ghostty".into(), "/Applications/Ghostty.app".into())
            ])
        );
        assert_eq!(parse_login_items("Handy, missing value"), Some(vec![("Handy".into(), String::new())]));
        assert_eq!(parse_login_items(""), Some(vec![]));
        assert_eq!(parse_login_items("a, b, c"), None);
    }

    #[test]
    fn ids_are_checked() {
        let h = Path::new("/Users/me");
        assert!(parse_id(h, "userAgent:/Users/me/Library/LaunchAgents/x.plist").is_ok());
        assert!(parse_id(h, "userAgent:/etc/x.plist").is_err());
        assert!(parse_id(h, "globalDaemon:/Library/LaunchDaemons/x.plist").is_err());
        assert_eq!(parse_id(h, "loginItem:Foo").unwrap(), (Kind::LoginItem, "Foo".to_string()));
    }
}
